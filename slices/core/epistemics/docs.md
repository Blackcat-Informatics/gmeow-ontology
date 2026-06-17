<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# epistemics

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/epistemics` · **tier: core**

Propositional epistemic relations — an agent's attitudes toward propositions/claims: belief, doubt,
suspension, pragmatic acceptance, and non-factive knowledge-that. The propositional companion to the
cognition slice (objectual, agent → entity): epistemics relates an agent to a **proposition**, not to
an entity. This slice provides both the flat doxastic spine and the reified tier: `gmeow:Proposition`,
`gmeow:DoxasticState` (a `kernel:MentalMoment`), `gmeow:credence`, `gmeow:doxasticClaim`
(linked to a `standpoint:StandpointClaim`), and `gmeow:DoxasticTenure` (a `temporal:TimeScopedRelation`).
Justification grounds land in a sibling child (#561).

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

## Terms

The truth-apt content class and the flat doxastic spine this minimal core seeds —
domain `gmeow:Agent`, range open (Principle 13).

### gmeow:Proposition

The truth-apt content an attitude is taken toward — the sibling of Goal by
direction of fit: a belief and a goal may share the same `gmeow:Proposition` but
differ in fit (mind-to-world vs world-to-mind), with deliberately no
`Goal ⊑ Proposition` subsumption.

### gmeow:believes · gmeow:knowsThat

The base attitude and its factive-looking refinement: `gmeow:believes` holds the
proposition true, and `gmeow:knowsThat rdfs:subPropertyOf gmeow:believes` is the
keystone — knowledge entails belief, never the reverse. There is no factive
`isTrue` axiom; `knowsThat` is a vantage-indexed claim that belief, truth-per-frame,
and justification hold, with the JTB judgement left to the solver.

### gmeow:doubts · gmeow:suspendsJudgementOn · gmeow:accepts

The non-believing stances: `gmeow:doubts` holds the proposition in low-credence
doubt; `gmeow:suspendsJudgementOn` is agnostic withholding (neither believes nor
disbelieves); `gmeow:accepts` is a pragmatic working premise that entails neither
belief nor truth.

## Dependencies

Depends on `kernel` (`gmeow:SocialObject`, `gmeow:Agent`, `gmeow:MentalMoment`), `temporal`
(`gmeow:TimeScopedRelation`, `gmeow:duringInterval`, `gmeow:TimeInterval`), and `standpoint`
(`gmeow:StandpointClaim`, `gmeow:claimModality`, `gmeow:StandpointModality`). The flat
`believes → accordingTo` bridge is documentation only at this tier; the reified tier hard-depends on
standpoint and temporal.

## Reified doxastic tier

When credence, temporal scope, or the mental moment itself must be first-class, promote from the flat
spine to a `gmeow:DoxasticState`.

| Term | Role |
|---|---|
| `gmeow:DoxasticState` | The agent's intrinsic believing mode — a `kernel:MentalMoment`. |
| `gmeow:epistemicAgent` | The believer (functional, domain `DoxasticState`, range `Agent`). |
| `gmeow:doxasticContent` | The believed `gmeow:Proposition` (functional). |
| `gmeow:doxasticClaim` | Links to a `standpoint:StandpointClaim` carrying the qualitative `claimModality`. |
| `gmeow:credence` | Graded degree-of-belief `[0,1]` as `xsd:decimal` (non-functional). |
| `gmeow:DoxasticTenure` | Time-scoped belief-revision history — a `temporal:TimeScopedRelation`. |
| `gmeow:tenureOfDoxasticState` | The `DoxasticState` whose holding interval the tenure records (functional). |

### `revise_belief` — suppression, not deletion

Revising a belief closes the prior `gmeow:DoxasticTenure` by setting `gmeow:endedAtTime` on its
interval and marks the **tenure** `gmeow:displayable false`. The original `gmeow:DoxasticState` is
retained as audit. A new `DoxasticState` (and a new open `DoxasticTenure`) records the revised belief.
This is the same suppression pattern used by `inference:InferenceTenure` (Principle 10).

Example: see `slices/core/epistemics/examples/belief-revision.ttl`.
