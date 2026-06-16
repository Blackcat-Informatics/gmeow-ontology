<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# cognition

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/cognition` · **tier: core**

An agent's **cognitive relations to a subject** — how an agent stands toward the
things it knows, attends to, and grasps. The objectual companion to the
`standpoint` slice (which carries *propositional* attitudes toward claims):
cognition relates an agent to an **entity**, not to a proposition. This minimal
core seeds the slice with a single axis — the knowledge spectrum. Belief,
justification, and knowledge-*that* are the sibling `epistemics` slice's job, not
this one.

## The knowledge spectrum

Four **ordinal** levels of an agent's epistemic depth toward a subject, chained
by `rdfs:subPropertyOf` so that each deeper level **entails** every shallower one:

| Depth | Property | Sense |
|---|---|---|
| 1 · faintest | `gmeow:isAwareOf` | has encountered it; knows it exists |
| 2 | `gmeow:knowsAbout` | knows facts about it; can describe it |
| 3 | `gmeow:understands` | comprehends it; can reason with and apply it |
| 4 · deepest | `gmeow:hasMastered` | expert command; can extend, teach, innovate on it |

```text
hasMastered ⊑ understands ⊑ knowsAbout ⊑ isAwareOf
```

### Why a subproperty chain (the reasoning value)

One deep assertion materialises the whole tail. `hasMastered(p, wd:Q28865)`
entails `understands`, `knowsAbout`, and `isAwareOf` of the same subject — so a
query "who is even *aware* of Python?" returns the dabbler and the master alike,
and "who *understands* it?" returns the masters too. The order is what reasoning
needs; the level boundaries are deliberately **vague** and no crisp cutoff is
claimed. The chain encodes the ordering, not a partition.

### Comprehension is not competency

The spectrum is the **knows** axis. `gmeow:hasSkill` (the `expertise` slice) is
the **can-do** axis. A skill entails *knowing about* its subject
(`gmeow:hasSkill ⊑ gmeow:knowsAbout`, asserted by `expertise`), but neither
*understanding* nor *mastery* of it — the axes stay orthogonal and are never
silently bridged (Principle 9).

### Standpoint indexing

Whose knowledge it is, and how deep, is a vantage-indexed claim through the
statement layer (Principle 9). Knowledge attributed to an agent by an observer
and knowledge avowed by the agent coexist; a contested depth is two coexisting
claims, never a global verdict.

## The mental-moment family

`gmeow:CognitiveState` — the agent-side **knowing** mode — sits under the kernel's
`gmeow:MentalMoment` umbrella (`gmeow:MentalMoment ⊑ gufo:IntrinsicMode`). That
umbrella gathers an agent's whole mental life under one queryable parent:

```text
gmeow:MentalMoment              (kernel · gufo:Category ⊑ gufo:IntrinsicMode)
├── gmeow:CognitiveState        (cognition · knowing)
├── doxastic states             (epistemics · believing — planned)
└── gmeow:IntentionalMode       (teleology · desiring/intending)
```

A consumer (the agent-memory flagship, Principle 15) can ask for *every* mental
moment of an agent at once, rather than walking three unrelated branches.

## The reified tier — KnowledgeProficiency

The flat spectrum is the 80% surface; promote to the reified
`gmeow:KnowledgeProficiency` (a `gufo:Relator`, mirroring `expertise`'s
`SkillProficiency`) when **level**, **scale**, **temporal scope**, or **standpoint**
matters (Principle 4). It binds `{agent} × {subject} × {level} × {scale} × {interval}`
through five roles (`knowledgeProficiencyAgent` / `…Subject` / `…Level` / `…Scale`
/ `…Interval`).

- **Mode vs relator, never double-typed.** A `gmeow:CognitiveState` (the knowing
  mode) and a `gmeow:KnowledgeProficiency` (the reified relator) for the same
  knowing relation are **different individuals**. "Founded on" is documentation,
  not an axiom (Principle 12).
- **The depth axis.** `gmeow:KnowledgeLevel` is an open, ordered value vocabulary
  (`knowledgeAware ≺ knowledgeKnowsAbout ≺ knowledgeUnderstands ≺ knowledgeMastered`),
  the kernel `GranularityLevel` idiom. `gmeow:deeperThan` is transitive **on levels
  only** — `KnowledgeProficiency` relators are never ordered by it.
- **`gmeow:pairsWith`** wires each flat spectrum property to the relator so
  `gmeow describe` can render the flat-first / reify-on-demand pairing.
- **Suppression, not deletion (Principle 10).** Lapsed knowledge is a *closed*
  `knowledgeProficiencyInterval` and/or `gmeow:displayable false` — retained, never
  deleted: what an agent knew *when* stays a query.

## Attention, interest, and objectual memory

Beyond *what it knows*, an agent records what it attends to, is curious about, and
remembers about a subject — the salience/recall surface for the flagship (#557):

| Relation | Sense |
|---|---|
| `gmeow:attendsTo` | directed attention / salience toward a subject |
| `gmeow:interestedIn` | sustained motivational orientation |
| `gmeow:curiousAbout` ⊑ `interestedIn` | knowledge-seeking interest |
| `gmeow:remembers` ⊑ `isAwareOf` | objectual memory-*of* a subject |

Boundaries, documented but **never bridged by axiom** (Principle 9):
attention/interest is an attentional pull, **not** a `teleology` goal; objectual
`gmeow:remembers` (a remembered *subject*) is distinct from the propositional
`gmeow:MemoryItem` (a remembered *claim*, the `ai` slice) — the link rides
`gmeow:memoryOf` as prose, so cognition adds **no** dependency on `ai`. Forgetting
is suppression (`gmeow:displayable false`), never deletion (Principle 10).

## Alignment

`schema:knowsAbout` aligns by `skos:exactMatch` to `gmeow:knowsAbout` — the
eponymous level is the bridge. A flat `schema:knowsAbout` edge lifts to the
asserted `gmeow:knowsAbout` level (its honest floor: you cannot infer mastery
from a flat edge). Going *down*, all four levels collapse to the single
`schema:knowsAbout` the wider world offers — a lossy projection of the spectrum.

### The alignment ledger — alternate depth frameworks

The native `gmeow:scaleKnowledgeDepth` is the default scale; Bloom's revised
taxonomy (`gmeow:scaleBloomRevised`), SOLO (`gmeow:scaleSOLO`), and Dreyfus
(`gmeow:scaleDreyfus`, from `languages`) are reusable alternate
`gmeow:ProficiencyScale`s — **no canonical framework is enforced** (Principle 6).
The band correspondence below is a *soft, documented* alignment (this ledger), never
an OWL axiom; the knows-axis and the can-do-axis are never silently bridged (P9):

| `gmeow:KnowledgeLevel` | Bloom's revised | SOLO | Dreyfus (knows-side reading) |
|---|---|---|---|
| `knowledgeAware` | Remember | unistructural | novice |
| `knowledgeKnowsAbout` | Understand | multistructural | advanced beginner |
| `knowledgeUnderstands` | Apply / Analyze | relational | competent / proficient |
| `knowledgeMastered` | Evaluate / Create | extended abstract | expert |

A `gmeow:KnowledgeProficiency` records its level on *whichever* scale it cites; the
correspondence above is for cross-framework reading, not automatic conversion (that
is solver-side, Principle 12).

## See also

- `expertise` — the applied-competency axis (`hasSkill`, `SkillProficiency`,
  occupations, credentials); `hasSkill ⊑ knowsAbout` bridges the two. The
  `gmeow:ProficiencyScale` / `gmeow:ProficiencyLevel` value vocab it once held now
  lives in `kernel` (relocated by #556 to break a dependency cycle).
- `kernel` — `gmeow:MentalMoment` (the shared mental-state umbrella) and the
  relocated proficiency value vocab.
- `teleology` — `gmeow:IntentionalMode` (desiring/intending), the conative member
  of the mental-moment family.
- `standpoint` — propositional attitudes toward claims.
- `ai` — `gmeow:MemoryItem`, the propositional remembered-*claim* construct
  `gmeow:remembers` is documented (not axiomatised) to bridge to.
- `epistemics` (planned) — belief, justification, knowledge-*that*.
