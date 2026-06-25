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
| [`INHABITED-CONSUMER.md`](INHABITED-CONSUMER.md) | configuration | the Principle 15 consumer, the placement decision, the settled-decisions ledger, the eventual slice anatomy |
| [`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md) | appendix | the three sources and the by-reference externals — staged for the `metadata/references.ttl` ledger |

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

`inhabitation:` (authored in the `slices/core/inhabitation` module) supplies that topology. It mints
the smallest vocabulary the gap genuinely requires — a durable-subject role, an inhabitation relator,
an embodiment, a transition event, a locus axis — and reuses everything else. The disciplined count
is roughly five new terms, not the eleven a naïve reading of the source material would mint, because
five of the "missing" categories are already present as roles, relators, events, and tenures over the
existing agent spine.

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

This generality is not scope creep; it is the **correctness proof.** An inhabitation ontology that
can model only AI sessions is overfit to one substrate. One that can model possession, incarnation,
and a corporation's officers — *faithfully, asserting no metaphysics* — has found the actual joints
of the relation. The contemplative traditions in particular are mature inhabitation ontologies,
refined over centuries; they made distinctions modern computing never did, and GMEOW borrows those
distinctions as structure ([`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)). The super-ontology
doctrine demands exactly this reach: model the relation once, maximally, and let every domain be a
profile of the one canonical form.

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
- **Principle 10 (suppression, never erasure).** A retired embodiment, a closed inhabitation, a
  banished tenure — all are suppressed, never deleted. Banishing ends a tenure; it does not destroy
  the record.
- **Principle 11 (frame-relativity).** Every inhabitation interval carries its temporal frame.
- **Principle 12 (the solver boundary).** Control among co-tenant subjects, session ordering, the
  scene-graph view, and the active memory view are *computed*, not asserted as triples.
- **Principle 14 (grounded agent memory is the flagship).** Inhabitation is the ontological backbone
  of the flagship claim that an agent's memory *"survives across sessions, models, and vendors."*
  Without it, that phrase is a slogan; with it, "the same subject before and after the model upgrade"
  is a query. The GTS `ai-package` and the MCP store/recall/revise triad are this set's named
  consumer (Principle 15).
- **Principle 17 (the logic is canonical).** Every minted term carries its `logic:` stereotype; the
  durable subject is an anti-rigid role, the inhabitation is a relator, the transition is an event —
  chosen so the foundation's rigidity and disjointness gates stay green.

## End state

The end state is a single, small, domain-general module:

- a durable `DigitalSubject` modeled as an **anti-rigid role an agent plays**, never a fourth rigid
  Kind colliding with the `Person ⟂ Organization ⟂ SoftwareAgent` partition;
- an `Inhabitation` **relator** — a lean spine carrying subject, host, interval, and locus, and
  *referencing* the independently-identified persona, embodiment, deployment, and memory view;
- `Embodiment` as the projected surface (subsuming Cagle's Avatar and the Sanskrit *avatāra* it
  descends from), and `Portal` as the transition between inhabitations;
- the subject → expression → embodiment manifestation spine **aligned by reference** to the existing
  WEMI spine and to the Trikāya parallel, never reinvented;
- the AI runtime stack as one profile, the spiritual / fictional / legal cases as the others, all
  sharing the one relator;
- subject-continuity carried as a contestable `counterpartOf` claim (never `owl:sameAs`) at the
  ontological layer and as a COSE signature at the cryptographic layer — two independent guarantees;
- and not one metaphysical claim asserted outside a named standpoint.

This makes inhabitation match the rest of the project: a maximal, domain-general model; maximal reuse
of what already exists; explicit projection of every contested or computed view; and the durable
digital subject treated, at last, as a first-class subject of its own existence.
