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

## Alignment

`schema:knowsAbout` aligns by `skos:exactMatch` to `gmeow:knowsAbout` — the
eponymous level is the bridge. A flat `schema:knowsAbout` edge lifts to the
asserted `gmeow:knowsAbout` level (its honest floor: you cannot infer mastery
from a flat edge). Going *down*, all four levels collapse to the single
`schema:knowsAbout` the wider world offers — a lossy projection of the spectrum.

## See also

- `expertise` — the applied-competency axis (`hasSkill`, `SkillProficiency`,
  occupations, credentials); `hasSkill ⊑ knowsAbout` bridges the two.
- `standpoint` — propositional attitudes toward claims.
- `epistemics` (planned) — belief, justification, knowledge-*that*.
