<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Work Orchestration — durable work clusters as one binding of the enactment kernel

Continuing work is not a queue of tasks that are created, completed, and forgotten; it is a durable
identity that outlives every occurrence of the work it governs. This slice owns that identity — the
work cluster whose goal never closes, whose prescription is revised only by supersession, whose
schedule keeps generating occurrences, and whose accumulated guidance, notes, and prior enactments are
the reason the next occurrence is better than the last. Everything about lifecycle, refinement, and
external-effect commitment is general and lives in the `logic:` enactment kernel; this slice binds
that kernel to the goal, norm, calendar, note, evidence, preference, and versioning spines, and mints
the operator review surface that explains every action it shows.

## What this slice owns

The domain-typed edges, and only those. The `logic:` grounding slice takes **no** axiom edge into
`gmeow:` — it is the bottom of the dependency graph and `gmeow:` is a generated lossy projection of it
— so every binding from a kernel term to a `gmeow:` class is authored here, in the direction the
architecture already uses (`gmeow:X rdfs:subPropertyOf logic:Y`, with a `gmeow:` range on the
`gmeow:` side).

- **`gmeow:WorkCluster`** — the continuing identity: its goal, its governing norm, its schedule, its
  single active prescription version, its guidance sets, its retained notes, and its enactment
  history.
- **`gmeow:GuidanceSet`** — versioned guidance, rubrics, and checklists, versioned through the
  existing version-membership apparatus so improving guidance never rewrites what governed a past
  decision.
- **The frontier bindings** — the kernel's derived actionable frontier bound into the shipped
  preference apparatus: candidate sets, comparison contexts, and the hard-failure triad behind blocked
  labels. Ties and incomparability are preserved; no universal winner is fabricated.
- **The context-assembly binding** — exactly one `logic:ContextAssembly` per enactment, recording both
  what was surfaced and what was excluded.
- **`gmeow:WorkReviewProjection`** — the comprehensive operator surface, with a projection receipt
  carrying exactly one preservation judgment and, for any non-exact judgment, a named expressiveness
  boundary.

## The design set

| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| [`design/WORK-ORCHESTRATION.md`](design/WORK-ORCHESTRATION.md) | charter | **design-only** — the charter is normative; the vocabulary it governs is authored in `module.ttl` against it | the continuing-cluster thesis, the binding discipline, guidance versioning, context assembly, recurrence non-identity, decision-support bindings, attribution, and the operator review projection with its Principle-17 preservation judgment |

The general kernel this slice binds is charted in
[`LOGIC-ENACTMENT.md`](../../grounding/logic/design/LOGIC-ENACTMENT.md) — the prescription →
enactment → commitment thesis, the seven orthogonal lifecycle axes and their transition and liveness
laws, the knowledge order over external-effect commitment, hierarchical refinement, and the reuse
ledger.

## The load-bearing distinctions

- **A cluster is not a plan and not a run.** Its prescription is a `logic:PrescriptionVersion` over a
  `logic:Plan`; each occurrence generates a `logic:Enactment`. The cluster's own identity is the
  continuing goal together with the standpoint that holds it.
- **An enactment is not an occurrence.** The schedule is the generator, not the generated. A
  deliberately unenacted occurrence is a recorded schedule exception, never a missing enactment.
- **Repeat, resume, and revise are three different things.** A repeat mints a new enactment against a
  new input snapshot; a resume advances the same enactment through an identity-gated restore; a revise
  supersedes the prescription and changes nothing about what an in-flight enactment is doing.
- **Guidance is advice, never authority.** The authority-separation gate the preference vocabulary
  already ships is generalized rather than duplicated; a second gate for the most safety-relevant rule
  in the surface would be a second source of truth.
- **Every contribution is attributed.** Model, operator, and tool are a closed value set with a
  mandatory binding, because an unattributed contribution in a review record is indistinguishable from
  a human judgment.
