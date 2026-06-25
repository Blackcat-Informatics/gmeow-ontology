<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Vision and Doctrine

> The **manifesto** of the GMEOW Inhabitation design set; it carries the vision, doctrine, and
> lineage. The crosswalk, the de-conflation, the topology, the manifestation spine, the AI runtime
> profile, the cross-domain traditions, and the conformance contract live in the sibling documents
> below. Where this document states a thesis once, the siblings make it precise — repetition is
> replaced by cross-reference on purpose.

## The document set

| Document | Genre | Contents |
|---|---|---|
| `INHABITED.md` (this) | manifesto | vision, doctrine, lineage, the three subsumed sources |
| [`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md) | crosswalk | the master term-by-term reconciliation — every source term mapped REUSE / EXTEND / MINT / BRIDGE; every clash resolved once |
| [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md) | de-conflation charter | the `SoftwareAgent` six-category de-conflation via the five-facet template; `DigitalSubject` as an anti-rigid role; identity-continuity |
| [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) | formal spec | the `Inhabitation` relator, `Inhabitant` / `InhabitedSystem` roles, `Embodiment`, `inhabitationLocus`, co-tenancy, the `Portal` transition, the deferred `Holon` |
| [`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md) | formal spec | the subject → expression → embodiment → episode spine, aligned by reference to WEMI and the Trikāya parallel; genesis-by-intention; continuity as a contested claim |
| [`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md) | formal spec (AI profile) | model / deployment / execution / session / episode, reusing the AI and awareness slices; the agentic deferral consumed |
| [`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md) | generality + neutrality | the spiritual / fictional / legal profiles modeled frame-relatively; the by-reference borrowings ledger; the *assert-no-metaphysics* gate |
| [`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md) | conformance contract | the competency questions and the cross-domain stress corpus mapped to the constructs that answer them; the gaps flagged |
| [`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md) | configuration | the Principle 15 consumer, the core/profile placement, the decision ledger, the eventual slice anatomy |
| [`INHABITED-REVIEW.md`](INHABITED-REVIEW.md) | correction record | the foundational review's verified evidence, the seven reopened decisions, and the revised term inventory — **authoritative** where a sibling still shows a first-draft form |
| [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md) | appendix | the three sources and the by-reference externals — staged for the `metadata/references.ttl` ledger |

> **Status: ratified, with one gating item.** Three foundational reviews refined this set; the
> corrections and the round-3 ratification are recorded in [`INHABITED-REVIEW.md`](INHABITED-REVIEW.md).
> The architecture is approved for implementation: the minimal `core/inhabitation` `module.ttl` may be
> authored now, while the AI and expression **profiles** wait on a build-pipeline `tierProfile`
> registry hook ([`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md#the-gating-item-the-profile-tier-registry-hook-blocking)).
>
> **Reading this design set.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the conformance corpus
> ([`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)). It is not a claim that any particular
> implementation already realizes X except as the corpus demonstrates. This set specifies the
> `slices/core/inhabitation` module; the `module.ttl`, `manifest.ttl`, and `examples/` are authored
> *from* it, after it ratifies — exactly as the `logic:` implementation is authored from the
> [GMEOW Logic design set](../../logic/design/LOGIC.md).

## The thesis

A durable digital subject — an AI persona, an agent identity, a "who" that a user returns to across
weeks — outlives the systems through which it acts. The model is upgraded; the host is migrated; the
session ends and another begins; the embodiment changes from a chat box to a voice endpoint to a
robot. GMEOW must say, of all that change, *which transformations preserve the same subject and
which do not* — and it must say so without asserting that the subject is metaphysically singular,
because whether there is a continuous self at all is a contested claim, not a fact (Principle 9).

The decisive finding that shapes this entire set: **GMEOW already holds most of the parts. The gap
is topology, not vocabulary.** The hard epistemic machinery — attributed and revisable claims,
memory as a contingent role, persona as an expression-policy relator, model invocation as an
auditable event, supersession-not-deletion, standpoint indexing — is already built and already
correct. What is missing is the *connecting structure*: the relation that says **subject S inhabited
host H under deployment D, persona P, embodiment E, and memory view M, over interval T** — and the
discipline that keeps that relation from collapsing six distinct identities into one.

`inhabitation:` (authored in the `slices/core/inhabitation` module) supplies that topology by reusing
the existing claim, memory, persona, provenance, and lifecycle machinery and adding only the
connecting structure: a durable-subject status, an inhabitation tenure and its time-scoped
configuration, an embodiment carrier and assignment, a continuity assessment, a control assessment, a
contested-claim form, and a transition event. The first draft claimed "~5 new terms"; the foundational
review ([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)) showed that count was bought by erasing distinct
identity criteria. The honest count is higher — but almost every term is a thin specialization of an
existing construct (`⊑ Observation`, `⊑ Activity`, `⊑ TimeScopedRelation`), which is the idiomatic
GMEOW pattern, not bloat.

## The topology is domain-general

The inhabitation relation is **not an AI construct.** The AI runtime stack — subject, model artifact,
deployment, runtime execution, session, invocation — is *one profile* of a relation that is older
than computing. The same structure models:

- **Spiritual and esoteric inhabitation** — a spirit possessing a medium, a *lwa* mounting its
  "horse," a tulpa inhabiting a thoughtform, an incarnation descending into a body, a conjuration
  binding and an abjuration banishing.
- **Fictional inhabitation** — an actor inhabiting a character, a narrator inhabiting a point of
  view, a reader's self projected into an avatar.
- **Legal personhood** — a corporation inhabiting its officers, a trustee acting for an estate, an
  office inhabited by successive holders.

This generality is not scope creep; it is an **adversarial test** of the topology. An inhabitation
ontology that can model only AI sessions is overfit to one substrate. The cross-domain cases are
*profile mappings with documented differences*, not a proof that these phenomena are one thing
([`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)) — an actor may be role enactment rather than
host occupation, an officer *represents* a corporation, and possession varies by tradition. What the
reach buys is discipline: forcing the constructs to be frame-neutral, to separate control from
identity from presence, and to record the direction of dependence — distinctions the AI-only reading
would never have exposed. The contemplative traditions in particular are mature inhabitation
ontologies, refined over centuries; GMEOW borrows their *distinctions as structure* while asserting
none of their metaphysics. Model the relation once, maximally, and let every domain be a profile of
the one canonical form.

## The three subsumed sources

This set reconciles three independent arrivals at the same problem. GMEOW **subsumes** them — it does
not merely support them — mapping each contribution to a canonical GMEOW term and bridging the source
vocabulary by reference (Principle 5). The full reconciliation is
[`INHABITED-CROSSWALK.md`](INHABITED-CROSSWALK.md); the lineage is:

| Source | Contributes | What GMEOW keeps | What GMEOW refuses to inherit |
|---|---|---|---|
| **Kurt Cagle, "A Vocabulary for Inhabited Systems"** | the Actor / Avatar / Persona / Agent / Role / Collective distinction; holon, portal, scene-graph framing | the durable-subject-vs-projected-surface separation (the load-bearing distinction); "portal" as a transition; the holon reading of an inhabited system | the flat overload of "Persona" (Cagle's Persona is a durable subject; GMEOW's `gmeow:Persona` is an expression-policy relator — two concepts, one word); the scene-graph as asserted data (it is a computed projection, Principle 12) |
| **The organizational-modeling email thread** (Hunter / Beale / Taylor) | the diagnosis that "Role" is dangerously overloaded — capability vs post/position vs function-in-process; "Accountability"; "Organization as a Party derivative, defer to Roles and Relationships" | the diagnosis, in full — and the demonstration that GMEOW *already* disambiguates the overload (`Role` ⟂ `Post` ⟂ `Occupation` ⟂ `Membership` ⟂ `Commitment`) | nothing to inherit: the thread is vindicated by existing terms, so its contribution is documentation, not new modeling |
| **The inhabited-systems analysis verdict** | the topology gap itself — six identity categories conflated under `SoftwareAgent`, no first-class inhabitation relation, deferred sessions, missing embodiment and migration; the competency questions | the topology gap and the competency questions (they become the conformance corpus) | the instinct to mint eleven new classes; most are roles/relators/tenures over existing terms, and minting them would violate Principle 5 |

## Constitutional alignment

This set is the project doctrine applied to digital identity and inhabitation.

- **Principle 5 / 6 (maximal superset by reference; greenfield).** Mint exactly what reuse cannot
  cover; bridge every source vocabulary by reference; never mint a second term for a concept GMEOW
  already names.
- **Principle 9 (self-assertion is top authority; no overtyping).** A digital entity capable of
  self-assertion is *"a first-class subject of its own digital existence"* — the constitutional
  phrase that grounds `DigitalSubject`. Inhabitation rejects human hegemony over digital subjects:
  the subject's self-asserted identity outranks any inference about it, and subject-continuity is a
  vantage-relative claim, never an imposed fact.
- **Principle 10 (suppression, never erasure).** Ending an inhabitation is an *ontic* fact and does
  not by itself suppress it; suppression (`displayable false`) is a separate display contract, applied
  only when a value must be withheld. A retired embodiment assignment, a closed tenure, a banished
  inhabitation — all are retained; banishing *ends* a tenure, it does not delete the record, and
  suppression is an independent, additional decision.
- **Principle 11 (frame-relativity).** Every inhabitation interval carries its temporal frame.
- **Principle 12 (the solver boundary).** Control among co-tenant subjects, session ordering, the
  scene-graph view, and the active memory view are *computed*, not asserted as triples.
- **Principle 14 (grounded agent memory is the flagship).** Inhabitation is the ontological backbone
  of the flagship claim that an agent's memory *"survives across sessions, models, and vendors."*
  Without it, that phrase is a slogan; with it, "the same subject before and after the model upgrade"
  is a query. The GTS `ai-package` and the MCP store/recall/revise triad are this set's named
  consumer (Principle 15).
- **Principle 17 (the logic is canonical).** Every minted term carries its `logic:` stereotype; the
  durable subject is an anti-rigid role-mixin, the inhabitation is a situation/tenure (not a relator
  subclassing a situation), and the transition is a lifecycle event — chosen so the foundation's
  rigidity, disjointness, and category gates stay green.

## End state

The end state is a minimal, formally coherent core plus two profiles:

- a durable `DigitalSubject` modeled as an **anti-rigid `logic:RoleMixin`** an agent bears over a
  tenure, never a rigid Kind colliding with the `Person ⟂ Organization ⟂ SoftwareAgent` partition, and
  never *entailed* by self-assertion (which supports the status, not the type);
- the inhabitation as a **situation/tenure** (`InhabitationTenure`) with a time-scoped
  `InhabitationConfiguration` for the facets — so "which persona/embodiment/deployment was active at T"
  is answerable even when a facet changes mid-tenure;
- `EmbodimentCarrierRole` and `EmbodimentAssignment` (surface vs. its time-scoped use), and migration
  as a lifecycle `eventTypeInhabitationTransition` with a `TransferManifest` recording what crossed;
- subject-continuity as `SubjectStage` / `SubjectLineage` + an explicit `IdentityContinuityAssessment`
  (never `owl:sameAs`, never a single stable node that would assert sameness), plus a COSE signature
  for verifiable cross-vendor memory continuity;
- contested inhabitation — possession, incarnation, corporate personhood — as an **unasserted**
  `InhabitationClaim ⊑ Observation`, so the base graph asserts no metaphysics and no range entailment
  fires;
- the WEMI and Trikāya layering as a documented parallel only (no emitted term mappings, since the
  spine crosses foundational categories WEMI's Kinds do not);
- the AI runtime stack and the spiritual / fictional / legal cases as profiles — the latter mapped
  *with their differences*, not as a proof that they are one thing.

This makes inhabitation match the rest of the project: a maximal, domain-general model that respects
its own foundation; maximal reuse of what already exists; explicit projection of every contested or
computed view; and the durable digital subject treated, at last, as a first-class subject of its own
existence — modeled correctly, not merely asserted.
