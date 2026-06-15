<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# teleology

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/teleology` · **tier: core**

Goals, desires, intentions, and commitments — the UFO-C intentional-moment
trichotomy surfaced as GMEOW core (the teleology design). Core by Principle 16
commitment: an agent recording *its own* goals across sessions is a question
every AI system will face about itself, alongside identity and deception
epistemics.

## The commitment-graded trichotomy

| Grade | Class | Grounding | Bound to |
|---|---|---|---|
| wanted | `gmeow:Desire` | `gufo:IntrinsicMode` | one agent (`intentBearer`) |
| internally committed | `gmeow:Intention` | `gufo:IntrinsicMode` | one agent (`intentBearer`) |
| socially committed | `gmeow:Commitment` | `gufo:Relator` | committed agent + distinct beneficiaries |

All three sit under the named umbrella **`gmeow:IntentionalMoment`** (UFO-C's
intentional moment), which exists so `intentionGoal` and `motivates` carry a
generator-visible domain — anonymous union domains vanish from the LinkML /
GraphQL / TypeScript surface (GraphQL / TypeScript surface review). All three aim at exactly one
**`gmeow:Goal`** (`intentionGoal`) — the
propositional content, a `SocialObject` describing a state of affairs,
satisfied by situations (`satisfiedBy`, vantage-indexed satisfaction). DOLCE
DnS arrives at the same description-satisfied-by-situations shape
independently; IAO's *objective specification* is the BFO-world counterpart.
Both are alignment targets (linkage-grade, never imported axioms), deferred
with the rest of the alignment set to keep this landing pure-ontology — the
target list is fixed in the alignment ledger: PROV `Plan`/`hadPlan`, P-Plan, FIBO FND-GAO,
IAO, SUMO `desires`/`intends`, CCO `Objective`, CRM P20/P21,
ConceptNet/ATOMIC; Wikidata goal **Q4503831** (verified 2026-06-11).

## Doctrine highlights

- **`counterGoal` is constitutive, not lexical** — the named shadow that
  partly defines the goal (an oath ↔ its betrayal). Symmetric, irreflexive
  (SHACL). Use `cn:Antonym`-grade opposition elsewhere.
- **No global satisfaction verdicts** (Principle 9): `satisfiedBy`,
  `motivates`, and goal-attribution all ride `accordingTo` on the statement.
  Avowed goals (agent's own vantage, top authority for its own standpoint)
  and attributed goals (observer vantage) coexist.
- **Flat-first** (Principle 4): `hasGoal` for the 80% case →
  Desire/Intention/Commitment when grade matters → `IntentionTenure`
  (the StandpointTenure idiom) when adoption/revision over time is the fact
  of interest. Revision by suppression, never deletion (Principle 10).
- **Solver boundary** (Principle 12): goal decomposition, planning, and
  means–end reasoning are never triples.
- **Deontic force lives in the norms extension**, which ranges
  `prescribedConduct` over `Goal` — dependency points extension → core only.
