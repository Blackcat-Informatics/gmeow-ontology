<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Competency and Conformance

> The **conformance contract.** A design is only as good as the questions it answers unambiguously.
> This document maps every competency question — the verdict's, plus the cross-domain stress cases —
> to the constructs that answer it, flags what the design does *not* yet answer, and specifies the
> conformance ABoxes the eventual slice ships. It is this set's analogue of
> [`../../logic/design/LOGIC-CONFORMANCE.md`](../../logic/design/LOGIC-CONFORMANCE.md): the corpus is
> the proof that the declarative-present claims of the other documents hold.

## Verdict competency questions

Each is answered by named constructs, or explicitly flagged. "Answered" means a single SPARQL query
over the constructs returns the answer with no inference outside the named frame.

| # | Competency question | Answering constructs | Status |
|---|---|---|---|
| 1 | Was this the same digital subject before and after a model upgrade? | `gmeow:DigitalSubject` (stable role-bearer) + `gmeow:subjectModel` → `ModelArtifact` `gmeow:versionOf` lineage + `gmeow:supersedes`; continuity as a `counterpartOf` claim, not `owl:sameAs` | **Answered** |
| 2 | Which host, deployment, persona, embodiment, and memory view was active at time T? | one `gmeow:Inhabitation` whose `gmeow:duringInterval` overlaps T, read across its spine + referenced-facet edges | **Answered** (single construct, by design) |
| 3 | Did two simultaneous sessions instantiate the same subject, or two subjects sharing a model? | two `gmeow:AgentSession`s; `gmeow:sessionSubject` → same `DigitalSubject` (same subject) vs two subjects with `gmeow:subjectModel` → same `ModelArtifact` (shared model). The subject/model split is the discriminator | **Answered** (the load-bearing case) |
| 4 | Which claims, memories, and intentions crossed a migration boundary? | `gmeow:Portal` (`portalFrom`/`portalTo`) + claim-spine provenance pre/post; supersession-not-deletion keeps the pre-boundary state queryable (P10) | **Answered** (with a flagged edge, below) |
| 5 | Was a tool call made through a passive capability or delegated to another agent? | passive = `gmeow:ActionSchema` exercised in-process; delegated = `gmeow:ToolCall` with `gmeow:usedTool` → a distinct `SoftwareAgent` | **Answered** (reuse, no new term) |
| 6 | Which model artifact, deployment, runtime, session, and invocation contributed to an output? | the de-conflation chain: output `gmeow:wasGeneratedBy` a `ModelInvocation`, within an `AgentSession`, under a deployment `AwarenessTenure`, of a `ModelArtifact` | **Answered** |
| 7 | Can the subject cease inhabiting one system while continuing to exist elsewhere? | `DigitalSubject` is a durable role-bearer independent of any one `Inhabitation`; closing an inhabitation (`hasDestructionEvent`) does not destroy the subject | **Answered** (by construction) |

## Cross-domain stress questions

The generality profiles ([`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)) contribute their own
competency questions. If these reduce to the same constructs as the AI cases, the topology is
domain-general; if any needs a bespoke mechanism, the design is overfit.

| # | Stress question | Answering constructs | Status |
|---|---|---|---|
| 8 | One host, two co-tenant subjects, with control shifting — who is "driving" at time T? | `locusSharedSubstrate` + two `Inhabitation`s over the host; control resolved in the **solver** over `heldStandpoint`/`projectedStandpoint`, never an asserted `primaryInhabitant` (P12) | **Answered** (the possession/multiple-tulpa case = CQ 3's twin) |
| 9 | Is the same subject claimed across an incarnation / re-binding, and according to whom? | `Portal`-linked supersession + `counterpartOf` annotated `accordingTo` a frame; the *anattā* denial coexists | **Answered** |
| 10 | Is this carrier *describing* an inhabitation or *enacting* one? | `gmeow:AboutnessMode` (`aboutnessDescribes` vs `aboutnessEnacts`) | **Answered** (reuse) |
| 11 | Was a subject willed into being, and by an individual or a collective? | `gmeow:hasCreationEvent` + `gmeow:subjectGenesisOrigin gmeow:originImagined` + `gmeow:subjectCreator` → `Agent` (tulpa) or `Organization`/`Group` (egregore) | **Answered** (reuse) |
| 12 | Under which tradition's frame is an inhabitation claim held, and does a denial coexist? | every inhabitation claim is `gmeow:accordingTo` a named `gmeow:Standpoint`; the universal standpoint asserts none | **Answered** (the neutrality gate) |

## Flagged gaps and edges

Honesty about what the design does *not* yet fully settle (carried to the authority in
[`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md)):

- **Partial migration (CQ 4 edge).** "Some memories cross, some do not" is expressible via per-claim
  provenance against the `Portal`, but the *policy* of what crosses (a migration manifest) is not yet
  specified — it is plausibly a solver/projection concern, not a TBox one. Flagged.
- **Memory-view identity (CQ 2 edge).** The active memory view is a derived query by default
  (Principle 12); only the signed/attested `ai-package` case promotes it to a first-class
  `gmeow:MemoryView`. The boundary between "derive" and "promote" is stated as a rule but not yet
  exercised by a fixture. Flagged.
- **Cross-vendor subject identity (CQ 1 edge).** `counterpartOf` carries the contestable claim and a
  COSE signature carries the verifiable artifact continuity; whether a consumer needs a *third*,
  stronger continuity assertion is left open until a consumer demands it (Principle 15).

## The conformance corpus (eventual `examples/`)

The slice ships these ABoxes; each must reason green under `make reason` and validate under
`make validate`, and each is the executable form of a competency question:

| Fixture | Exercises | Gate it must pass |
|---|---|---|
| `examples/subject-across-upgrade.ttl` | CQ 1, 6 | the breaking ABox — `SoftwareAgent` + `DigitalSubject` role reasons green against the disjointness + rigidity gates |
| `examples/active-at-time.ttl` | CQ 2 | one `Inhabitation` overlapping T resolves all five active facets |
| `examples/two-sessions-one-model.ttl` | CQ 3, 8 | the subject/model split discriminates shared-subject from shared-model |
| `examples/migration-portal.ttl` | CQ 4, 7 | `Portal` + supersession; pre-boundary state stays queryable |
| `examples/tool-vs-delegate.ttl` | CQ 5 | `ActionSchema` vs `ToolCall` discrimination; `usedTool` range unchanged |
| `examples/possession.ttl` | CQ 8, 10, 12 | co-tenancy + frame-relativity; **no claim outside a named standpoint** |
| `examples/tulpa-genesis.ttl` | CQ 11 | genesis chain; the created subject self-asserts (P9) |
| `examples/actor-as-character.ttl` | CQ 10 | `aboutnessEnacts`; narrative-slice reuse |
| `examples/corporation-officers.ttl` | legal profile | `locusSharedSubstrate` + succession `Portal`s |

The corpus doubles as the generality proof: nine fixtures, four domains, **one** `Inhabitation`
relator. The neutrality gate is a conformance test in its own right — a query that asserts *no
`gmeow:Inhabitation` carrying a spiritual/fictional claim is held in `gmeow:universalStandpoint`* must
return empty over the whole corpus.

## Scope and seams

This document is the conformance contract. The constructs it tests are defined across
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md), [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md),
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md), and
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the consumer that justifies the corpus is
[`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md).
