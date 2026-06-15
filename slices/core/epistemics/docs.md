<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# epistemics

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/epistemics` · **tier: core**

Propositional epistemic relations — an agent's attitudes toward propositions/claims: belief, doubt,
suspension, pragmatic acceptance, and non-factive knowledge-that. The propositional companion to the
cognition slice (objectual, agent → entity): epistemics relates an agent to a **proposition**, not to
an entity. This minimal core seeds the slice with the truth-apt `gmeow:Proposition` and the flat
doxastic spine; the reified tier (a doxastic state, credence, justification) and the mental-moment
grounding land in sibling children (#560 / #561 / #562).

## The flat doxastic spine

| Property | Stance |
|---|---|
| `gmeow:believes` | holds the proposition true (base attitude) |
| `gmeow:doubts` | holds it in doubt (low-credence, unsettled) |
| `gmeow:suspendsJudgementOn` | neither believes nor disbelieves (agnostic withholding) |
| `gmeow:accepts` | a working premise — entails neither belief nor truth |
| `gmeow:knowsThat` | justified, standpoint-true belief — `⊑ gmeow:believes` |

Domain `gmeow:Agent`, range **open** (Principle 13): the 80% case points cheaply at any reified
statement or claim; typed `gmeow:Proposition` content arrives at the reified tier.

## Doctrine highlights

- **The keystone** — `gmeow:knowsThat rdfs:subPropertyOf gmeow:believes`: knowledge entails belief;
  the reverse never holds; asserting `knowsThat` materialises `believes`.
- **No factive knows** (Principles 1, 12) — there is no `isTrue`, no truth datatype, no factivity
  axiom. `knowsThat` is a vantage-indexed claim that (belief ∧ truth-per-frame ∧ justification) holds,
  never a global verdict; the JTB → knowledge judgement (and Gettier defeaters) is solver work.
- **Truth-per-frame is reused, never re-minted** (Principle 6) — how settled a proposition is held
  rides the standpoint modality (`gmeow:standpointModality` on a flattened statement,
  `gmeow:claimModality` on a reified `StandpointClaim`), in the standpoint slice.
- **`believes → accordingTo` is documented, not axiomatised** — an `owl:ObjectProperty` cannot
  `rdfs:subPropertyOf` an `owl:AnnotationProperty` in OWL 2 DL; the `gmeow:vantage ⊑ accordingTo`
  precedent. Realised in the projection layer.
- **Proposition is the sibling of Goal, by direction of fit** — a belief and a goal may share the
  same `gmeow:Proposition` but differ in fit (mind-to-world vs world-to-mind); deliberately **no**
  `Goal ⊑ Proposition` subsumption, no `teleology` dependency.
- **Acceptance is not belief** — `gmeow:accepts` is pragmatic; it entails neither belief nor truth.

## Dependencies

Depends on `kernel` (`gmeow:SocialObject`, `gmeow:Agent`). The `believes → accordingTo` bridge to
`standpoint` is documentation only at this tier; `standpoint` becomes a hard dependency at #561.
