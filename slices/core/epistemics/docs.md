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
Justification is factored five ways in this module (`gmeow:hasAvailableEvidence`, `gmeow:basesBeliefOn`,
`gmeow:supportsUnder`, `gmeow:adequateUnder`, and defeat via `gmeow:defeatedBy` / `gmeow:hasDefeatStatus`);
full inferential argumentation lives in the inference slice.

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
- **No factive knows** (Principles 1, 12) — `gmeow:knowsThat` is **non-factive**. Asserting it
  entails only `gmeow:believes`; it commits no global truth verdict. There is no `isTrue`, no truth
  datatype, and no factivity axiom. `knowsThat` is a vantage-indexed claim that
  (belief ∧ truth-per-frame ∧ justification) holds from a frame; the JTB → knowledge judgement
  (and Gettier defeaters) is solver work.
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

## External alignment

`gmeow:DoxasticState` is axiomatically grounded in gUFO: it is a `gufo:Kind` under
`gmeow:MentalMoment`, and `gmeow:MentalMoment` is itself a named `gufo:Category` below
`gufo:IntrinsicMode` (see the kernel slice). That subsumption is ontology, not a mapping row:
the SSSOM set records cross-vocabulary links only.

The epistemics mapping set is authored in
[`slices/core/epistemics/mappings/equivalences.ttl`](./mappings/equivalences.ttl) and compiled to
`generated/mappings/gmeow-epistemics.sssom.tsv` (materialized by `make check`).
All alignments are by reference (Principle 5); GMEOW never imports an external axiom.

| GMEOW term | External target(s) | Predicate | Note |
|---|---|---|---|
| `gmeow:DoxasticState` | `crminf:I2_Belief` | `skos:closeMatch` | both are an agent's held attitude toward a proposition; CRMinf I2 is temporal, GMEOW's state is an endurant mental moment |
| `gmeow:Proposition` | `crminf:I4_Proposition_Set` | `skos:relatedMatch` | I4 is a set; GMEOW's term is a single truth-apt content |
| `gmeow:Proposition` | `iao:0000030` | `skos:relatedMatch` | IAO information content entity, broader |
| `gmeow:Proposition` | `wd:Q108163` | `skos:relatedMatch` | Wikidata "proposition", entity-page verified |
| `gmeow:believes` | `sumo:believes` | `skos:relatedMatch` | SUMO's predicate is conceptually adjacent; GMEOW's is non-factive and standpoint-indexed |
| `gmeow:knowsThat` | `sumo:knows` | `skos:relatedMatch` | explicitly **non-factive**: GMEOW `knowsThat` entails only `believes`, never global truth |
| `gmeow:basesBeliefOn` | `crminf:J2_concluded_that` | `skos:relatedMatch` | direction of support is inverted |

`gmeow:credence` and `gmeow:accepts` are left unaligned: no canonical, resolvable RDF vocabulary
was found. ATOMIC belief edges and AIF S-/RA-nodes are referenced in prose only; no stable,
resolvable namespace suitable for SSSOM rows is available. Epistemic-modal operators (S4/S5/KD45)
belong to the `logic:` slice profiles.

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
| `gmeow:EpistemicContext` | A doxastic context (belief-world) — where the believed propositions hold, the context `logic:doxasticallyAccessible` ranges over. DISTINCT from `gmeow:DoxasticState`: the DoxasticState is the believing mental moment that inheres in the agent; the EpistemicContext is the belief-world the attitude reaches. |
| `gmeow:epistemicAgent` | The believer (functional, domain `DoxasticState`, range `Agent`). |
| `gmeow:doxasticContent` | The believed `gmeow:Proposition` (functional). |
| `gmeow:doxasticClaim` | Links to a `standpoint:StandpointClaim` carrying the qualitative `claimModality`. |
| `gmeow:credence` | Graded degree-of-belief `[0,1]` as `xsd:decimal` (non-functional). |
| `gmeow:DoxasticTenure` | Time-scoped belief-revision history — a `temporal:TimeScopedRelation`. |
| `gmeow:tenureOfDoxasticState` | The `DoxasticState` whose holding interval the tenure records (functional). |

### Factored justification (the five-way split)

"Justification" is not one thing. Following the canonical `LOGIC-FOUNDATION.md` account, this slice
splits it into five independent components a single word usually hides — each non-functional, so
competing values coexist (Principle 9):

| Component | Edge | Range | Kind | Meaning |
|---|---|---|---|---|
| available evidence | `gmeow:hasAvailableEvidence` | `gmeow:JustificationGround` | asserted provenance | what the agent has access to — a modelling fact, never a solver judgement |
| basing | `gmeow:basesBeliefOn` | `gmeow:JustificationGround` | asserted provenance | which ground the belief is actually founded on (available ≠ used) — a modelling fact |
| support under a standard | `gmeow:supportsUnder` | `gmeow:SupportAssessment` | solver adjudication | how strongly the evidence warrants the content, relative to a named `gmeow:EpistemicStandard` (Principle 12) |
| adequacy | `gmeow:adequateUnder` | `gmeow:AdequacyAssessment` | solver adjudication | whether that support meets the standard's threshold (`gmeow:meetsThreshold` → `gmeow:AdequacyVerdict`) (Principle 12) |
| defeat | `gmeow:defeatedBy` / `gmeow:hasDefeatStatus` | `gmeow:Defeater` / `gmeow:JustificationStatus` | structural / solver adjudication | the structural defeater (what defeats — asserted) vs the solver-set verdict flag (the adjudicated outcome, Principle 12) |

The `gmeow:EpistemicStandard` vocabulary names the bar — `gmeow:standardOrdinary`,
`gmeow:standardScientific`, and the legal `gmeow:standardLegalPreponderance` /
`...ClearAndConvincing` / `...BeyondReasonableDoubt` — so the same basis may be adequate under one
standard and inadequate under a stricter one. The **defeat reconcile** keeps two distinct facts
apart: `gmeow:defeatedBy` names *what* does the defeating (an `inference:Argument`, a
`gmeow:StandpointClaim`, or a `gmeow:EvidenceSpan`), while `gmeow:hasDefeatStatus` carries the
solver's adjudicated verdict (`gmeow:justificationStatusGettier` / `...Defeated` / `...Undermined` /
`...Undercut` / `...Rebutted`, aligned with the inference slice's `gmeow:AttackKind`). Full argument
graphs and inference-making acts live in the inference slice.

### Locally-factive knowledge

The keystone `gmeow:knowsThat` is deliberately **non-factive** — a vantage-indexed claim entailing only
`believes`. Alongside it sits a **sibling** (never a subproperty), `gmeow:knowsThatIn`, which is
**locally factive**: knowing *P* in a belief-world *W* entails that *P* holds *in W*, never globally
across all worlds. This keeps factivity honest in a paraconsistent, world-indexed setting — an agent
can know contested facts in its own world without the model claiming them everywhere.

The reified promotion `gmeow:KnowledgeClaim` carries the four roles — `gmeow:knowerAgent`,
`gmeow:knownProposition`, `gmeow:knownInWorld` (→ `gmeow:EpistemicContext`), and `gmeow:underStandard`
(→ `gmeow:EpistemicStandard`). It is a **claim apparatus** (a `logic:Relator`, like `gmeow:ClaimToken`),
deliberately **not** a `kernel:MentalMoment` — the believing *attitude* stays `gmeow:DoxasticState`.
Local factivity is recorded by **prose only**: there is no `gmeow:isTrue`, no factivity axiom, and no
asserted `logic:` triple — the world-indexed rule lives in the logic slice over a world-relative hold
predicate, so the no-truth-bit invariant and the clean DL/EL profile both hold.

**Non-factive knowledge-attribution** is kept strictly separate. "They take themselves to know" is a
claim *about an attitude*, never the factive relation: `gmeow:claimsToKnowThat` and `gmeow:takesAsKnown`
(the knowledge analogue of `gmeow:accepts`) are flat, open-range, and never subproperties of the
doxastic or factive spine; `gmeow:KnowledgeAttribution` reifies (`gmeow:attributingAgent`,
`gmeow:attributedKnower`, `gmeow:attributedProposition`). Worked example:
[`examples/locally-factive-knowledge.ttl`](./examples/locally-factive-knowledge.ttl).

### `revise_belief` — suppression, not deletion

Revising a belief closes the prior `gmeow:DoxasticTenure` by setting `gmeow:endedAtTime` on its
interval and marks the **tenure** `gmeow:displayable false`. The original `gmeow:DoxasticState` is
retained as audit. A new `DoxasticState` (and a new open `DoxasticTenure`) records the revised belief.
This is the same suppression pattern used by `inference:InferenceTenure` (Principle 10).

Example: see `slices/core/epistemics/examples/belief-revision.ttl`.

## Flagship worked example

[`slices/core/epistemics/examples/flagship-epistemic-ledger.ttl`](./examples/flagship-epistemic-ledger.ttl)
is the reference epistemic ledger for this slice. It models an operator deciding whether a
flagship LLM recalled a meeting date correctly:

- `ex:propRecall` mints the shared `gmeow:Proposition`.
- The operator's original `gmeow:DoxasticState` carries `gmeow:credence "0.85"` and
  `gmeow:hasAvailableEvidence` / `gmeow:basesBeliefOn` an `EvidenceSpan` pinned to a calendar chunk.
- A defeater arrives; the operator revises to a lower-credence state and a new
  `gmeow:DoxasticTenure`. The original tenure is closed with `gmeow:endedAtTime` and
  suppressed with `gmeow:displayable false` (Principle 10); the old `DoxasticState` is
  retained as audit.
- The LLM itself is modeled with `gmeow:knowsThat` — non-factive: it entails only that the
  LLM `gmeow:believes` the proposition, with no global truth commitment.
- A skeptic `gmeow:doubts` the same proposition and records a standpoint-indexed refutation.
  The competing attitudes coexist; none is privileged (Principle 9).
