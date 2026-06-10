<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# The GMEOW Constitution

These are the principles every design decision, pull request, and release of GMEOW is
measured against. They are **normative**: where a choice conflicts with a principle below,
the choice changes to comply — or the principle is amended first, in the open. A principle is
never silently overridden. Cite them by number ("Principle 4") in issues and pull requests.

This document states *what* GMEOW commits to and *why*. It complements — and does not
duplicate — [`docs/RATIONALE.md`](./docs/RATIONALE.md) (the problems GMEOW solves) and the
exemplar modelling guides it points to.

---

## 1. SOTA by being SOTA

> **Model what *should* have been written; never accept a bad standard or a weak tool as an
> excuse to compromise quality.**

GMEOW is the bridge between the correct modelling of a concept and the compromises baked into
commonly-used, substandard vocabularies. When a surface vocabulary models something poorly,
GMEOW models it *correctly* and bridges to the weaker form by reference (Principle 5) — it
never inherits the weakness. We aim for GMEOW to be the substrate of choice for **grounded AI
memory and claim provenance first** (Principle 14) — and, through the same canonical core, for
high-quality knowledge graphs, scholarly and archival work, and inter-ontology linkage. The
linked-data and ontology communities are a constituency we serve and bridge to with full
seriousness; the AI ecosystem is the home market.

*Embodied in:* [`docs/names-mapping.md`](./docs/names-mapping.md),
[`docs/languages-mapping.md`](./docs/languages-mapping.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md).

## 2. RDF 1.2 / RDF\*-first — precisely scoped

> **Statement-level metadata — provenance, confidence, temporal scope — is authored as native
> RDF 1.2 / RDF\* and is the canonical source. The logical TBox stays OWL 2 DL.**

RDF-1.2-first governs the **statement-metadata layer only**. The decidable logical core
remains OWL 2 DL because RDF 1.2 triple-terms are not OWL 2 DL — and GMEOW never claims
otherwise. The positioning must never overclaim: "RDF-1.2-first" means the metadata layer, not
the ontology's logic.

*Embodied in:* the authored `statement-dsl/` source; [`README.md`](./README.md) § RDF 1.2.
*Tested by:* `gmeow compile-statements --check`, the RDF 1.2 round-trip tests.

## 3. The OWL axiom-annotation form is a generated, reasoning-lossless downcast

> **The `owl:Axiom` / `owl:annotatedSource·Property·Target` encoding is a *generated*
> compatibility projection of the RDF 1.2 source — lossless for reasoning, never a competing
> source of truth.**

GMEOW gates reasoning on OWL 2 DL tools (ELK, HermiT) that cannot yet consume RDF 1.2, so it
emits the plain-RDF form *for them* — the same lossy-compatibility-as-projection principle it
applies to schema.org / vCard / FOAF (Principle 4). It is the downgrade for legacy tooling, and
it recedes naturally as RDF-1.2-native reasoners and stores arrive. The canonical source never
changes.

*Embodied in:* `gmeow compile-statements`, `queries/rdf12-project.rq` (a codec between two
generated forms). *Tested by:* the OWL↔RDF 1.2 round-trip / isomorphism gate.

## 4. One canonical source; everything else a generated lossy projection

> **Every fact is authored once in the canonical core; all other forms are generated. Lossy
> compatibility lives in the projection, never in the canonical core.**

This is GMEOW's founding doctrine, applied uniformly — to surface-vocabulary exports, to the
alignment layer (the mapping compiler), and to the RDF 1.2 ↔ OWL relationship (Principles 2–3).
The reasoned core stays clean; lossiness is pushed to the boundary and made explicit.

*Embodied in:* [`docs/projections.md`](./docs/projections.md); [`README.md`](./README.md) §
The mapping compiler. *Tested by:* `gmeow compile-mappings --check`, projection round-trips.

## 5. Maximal superset, maximal bridging — by reference

> **Mint exactly one canonical term per concept and align it to every surface vocabulary by
> reference (SSSOM / EDOAL / FnO / SPARQL); never rewrite anyone else's data.**

Data already published in FOAF, schema.org, vCard, GEDCOM, DOAP, PROV-O, ORG, and Wikidata is
covered *by reference*, not by rewriting. Rich interlinking ships out of the box. Asserting a
link copies nothing; copying axioms in is license-gated and a reference-only source is refused.
This applies **recursively to the foundational spine**: gUFO is bridged by reference to BFO 2020
(ISO/IEC 21838-2) — link-only, never imported — so even GMEOW's upper-ontology grounding is
interoperable without inheriting anyone's axioms.

**Identity & coreference (one thing, many records).** The same doctrine governs *instance*
identity. A thing's identity is the **stable entity itself** — independent of the names, labels,
occupants, or external records attached to it (a place persists across a renaming; an extent
survives its building). Coreference to external records is asserted **by reference** —
`gmeow:authorityLink` + `skos:exactMatch` / `skos:closeMatch`, with a hub (Wikidata) reaching the
rest — never an `owl:sameAs` merge that would collapse contested or standpoint-indexed claims
into one.

*Embodied in:* [`docs/RATIONALE.md`](./docs/RATIONALE.md) § The solution; `mapping-dsl/`,
`mappings/*.sssom.tsv`; the foundational bridge
[`docs/foundational-bridging.md`](./docs/foundational-bridging.md).

## 6. Greenfield — get it right, not compatible

> **When replacing an element, pick the optimal solution and remove the inferior one; never
> retain a worse element for backwards-compatibility.**

The canonical core carries no backwards-compatibility debt — the easy or already-present
solution does not win on those grounds alone. Compatibility for *consumers* is provided
externally, by projection (Principle 4). Releases are immutable: defects are fixed forward in a
new version, never in place.

*Embodied in:* [`README.md`](./README.md) § Publishing (immutable releases).

## 7. Verified by construction

> **A generated artifact is trustworthy only if it is round-trip-checked and guarded by a
> no-drift `--check` in CI.**

Generation without verification is a second source of truth in disguise. Every downcast — the
mapping artifacts, the RDF 1.2 view, the OWL compat form — must be regenerable and proven
non-divergent, so drift is *impossible* rather than merely discouraged.

*Embodied in:* `gmeow compile-mappings --check`, `gmeow compile-statements --check`, the
`projection_lint` / `statement_lint` invariants, `make check`.

## 8. Reasoning-gated and FAIR

> **The logical core is OWL 2 DL, gated by ELK (fast) and HermiT (sound + complete); published
> FAIR with content negotiation, VoID/DCAT, a DOI, and LOD-Cloud presence. The reasoner is our
> quality assurance, never the consumer's prerequisite.**

A super-vocabulary is only useful if a reasoner can hold its union coherent and if the world can
find, dereference, and cite it. Reasoning and FAIR publication are first-order requirements, not
afterthoughts — but they are *verification and hygiene*, not the value proposition. The reasoner
catches modelling errors before any consumer sees them; no consumer is ever required to run one,
or to know that we do (Principle 13). FAIR publication is pursued seriously as scholarly bridge
and discoverability hygiene; the product it certifies is Principle 14's.

**Documentation is a first-class artifact.** Every GMEOW-namespaced class, property, annotation
property, datatype, individual, and ontology header must carry `rdfs:label`, `skos:definition`,
and `rdfs:isDefinedBy` so the vocabulary is human-readable and machine-discoverable by default.
This is enforced by the annotation-completeness gate (`make validate`) and is not waived for
generated artifacts (Principles 4 and 7).

*Embodied in:* [`README.md`](./README.md) § Reasoning, § Publishing; `make reason`,
`make explain`, `make metadata`, `make crossref`.

## 9. Inclusive without overtyping; anti-colonial in every direction; self-assertion is top authority

> **Identity, naming, and language are reified, co-equal facets drawn from open value
> vocabularies of individuals; there is no "primary"/"preferred" privileging, and a subject's
> self-asserted values — human *or* digital — are the highest authority, above any inference.**

Co-equal facets, not subclass explosions or forced enums; orthogonal axes are never inferred
from one another. There is no `primaryName` / `primaryGender`; display selection is
locale-relative and symmetric. A schema's *shape* can enact hierarchy — GMEOW structurally
refuses it.

**Coloniality is the imposition of a category onto a subject who did not assert it — and it
is not only historical, nor only one-directional.** GMEOW refuses it both ways. *Onto humans:*
the imposer may be a colonial power, a dominant platform, or — now — an AI system inferring or
minting a name, gender, or language *on a person's behalf*. *Onto digital entities:* GMEOW also
rejects **human hegemony over digital and AI entities** — an entity capable of self-assertion is
a first-class subject of its own digital existence, not an object for others to define. The
ontology is **forward-looking**: it learns from the mistakes of the past rather than re-enacting
them on new kinds of subject. Machine-derived (and human-imposed) values are recorded as exactly
that — attributed and confidence-weighted (Principles 2–3), never as ground truth; a subject's
own assertion outranks any inference about it. This is also why a `gmeow:Language` may be
AI-minted yet fully first-class, and why such provenance is always carried, never erased.

**The unified observation stance.** This generalises: *every* value is an **attributed, dated,
confidence-weighted, vantage-relative observation/claim**, never ground truth — a measurement, a
standpoint-indexed claim, and a sensory perception are the **same reified construct** (a claim made
*from a vantage*). And ontic **indeterminacy** is held distinct from epistemic confidence: a value
may be inherently crisp, vague, fuzzy, probabilistic, or disputed (`gmeow:Determinacy`), and that is
recorded explicitly rather than assumed away — distinct from *how sure we are* (`gmeow:confidence`).

*Embodied in:* [`docs/names-mapping.md`](./docs/names-mapping.md),
[`docs/languages-mapping.md`](./docs/languages-mapping.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md),
[`docs/standpoints.md`](./docs/standpoints.md) (no preferred/primary claim — a contested fact is
several coequal standpoint-indexed claims). *Tested by:* the 7-axis orthogonality matrix tests;
`tests/test_standpoint.py` (coexistence + no-preferred-claim guards).

## 10. Suppression, never erasure

> **A superseded label — a deadname, a former gender — is recorded with `gmeow:displayable
> false`: never displayed, never deleted.**

Self-determination requires both honouring the current self-assertion *and* preserving an
honest, auditable record. Suppression is a display contract enforced through projection
(Principle 4): the data is retained, the leak is prevented.

**Disclosure control by projection (the general mechanism).** Suppression is one case of a single
mechanism: **withhold or coarsen a value through the projection layer under a trigger, never by
deletion**. The trigger may be supersession (a deadname, a former gender, a withdrawn standpoint, an
expired right), or **access/consent** (privacy: a sensitive value — a person's precise location,
health — is redacted by *generalisation*, e.g. publishing a coarser region rather than exact
coordinates). Erasure is never the tool; the projection is.

*Embodied in:* `gmeow:displayable`, `fnSelectDisplayName` (withhold); `gmeow:coarsenTo` +
the `gmeow:GranularityLevel` axis, `fnCoarsenToGranularity` (coarsen — #72/#79, aligned to
`dpv:Generalisation`); [`docs/projections.md`](./docs/projections.md);
[`docs/identity-mapping.md`](./docs/identity-mapping.md);
[`docs/standpoints.md`](./docs/standpoints.md) (a withdrawn standpoint / closed
`gmeow:StandpointTenure` is suppressed, not deleted). *Tested by:* the projection
suppression tests; `tests/test_suppress_gen.py`; `tests/test_standpoint.py`.

## 11. Frame-relativity — values live in an explicit reference system

> **Every measured or expressed value is relative to an explicit reference system; separate
> frame-independent structure (topology) from frame-relative value (geometry).**

A coordinate, a date, a price, a mass, a colour, even a name are meaningless without the system
they are read in — a coordinate reference system, a unit, a currency, a calendar + timescale, a
colourspace, a language/register. GMEOW makes the frame **explicit and first-class** (a
self-describing reference-frame *Profile*), keeps the relational **structure** (containment,
adjacency, order) frame-independent, and treats the **value** (the coordinate tuple) as
frame-relative; conversion between frames is a computation, not an assertion (Principle 12). A
value asserted without its frame is ill-formed. This is also what makes the model *open*: a new
realm or system is a new frame filling a fixed profile, never a change to the core.

*Embodied in:* the generalised reference-frame facility (#70); the Location module (#42) for
spatial frames; the temporal module for calendar/timescale frames; mappings to QUDT/OM for
measurement frames, FIBO for currency frames, OWL-Time `time:TRS` for temporal reference
systems, and Lexvo for language frames.

## 12. Compute outside the logic — the solver boundary

> **The OWL 2 DL core holds structure, relationships, and canonical values; heavy computation
> lives in an external solver layer aligned by reference, never materialised as triples.**

The reasoned core stays decidable and small (Principle 8). Coordinate and datum transforms,
RCC-8 / Allen relation-algebra composition, trajectory interpolation, n-dimensional vector
operations, calendar/timescale conversion, and probabilistic / SLAM updates are **computed, not
asserted** — performed by purpose-built engines (a GeoSPARQL/GIS engine, a transform solver, a
vector store) the ontology points to **by reference**. GMEOW models the *logic* and projects it
losslessly (the standpoint precedent — model it, don't collapse it); it never turns the
triplestore into a calculator or bloats the TBox with derived geometry.

*Embodied in:* the projection layer; the lossless standpoint projections; the solver boundary of
the #42 locations epic. *Tested by:* the OWL 2 DL profile gate (ELK / HermiT) staying green as
expressivity grows.

## 13. The product is a tool; the ontology is its engine

> **GMEOW is adopted through tools, formats, and patterns — a pip-installable client, MCP
> tools, JSON/Pydantic schemas, a single-file package format. No consumer is ever required to
> learn RDF or OWL to benefit.**

Developers adopt tools and file formats; they do not adopt vocabularies. History is decisive on
this: modelling quality has near-zero correlation with ontology adoption, and GMEOW does not get
to be the exception by being better — it gets to be the exception by **not asking**. The flat
JSON, Pydantic, and MCP surfaces are the front door; the reasoned RDF core is the engine room.
Disclosure is progressive: the deep model — standpoints, frames, suppression — is discovered at
the moment it is needed (the first time two models disagree about a fact), never at minute one.

**The five-minute gate.** From `pip install` to storing and recalling one attributed,
confidence-weighted claim must take under five minutes, with no Docker, no reasoner, and no RDF
knowledge. This is a release gate, measured, not an aspiration. Every step of the toolchain that
is load-bearing for *us* (Principles 7–8) is friction for *them* — it stays behind the wall.

*Embodied in:* the `gmeow` PyPI client (v0.2.0 spec); `src/gmeow_tools/mcp_server.py`;
`dist/schemas/` (generated Pydantic / JSON Schema / TypeScript / GraphQL); `dist/llms.txt`;
the flat-JSON projections (#55). *Tested by:* the quickstart time-to-first-claim gate; the
schema round-trip tests.

## 14. Grounded agent memory and claim provenance are the flagship

> **An LLM output is a claim, not a truth — stored with provenance, evidence, confidence, and
> standpoint; recalled with filters; revised by suppression, never deletion. An agent's memory
> is a portable, signed, append-only package of such claims. This is the product.**

This operationalises the unified observation stance (Principle 9) and suppression (Principle 10)
for the ecosystem that needs them most and has them least. Today's agent memory is an
unattributed text or vector blob — no provenance, no evidence link, no confidence, no temporal
scope, no contestability, no audit of belief revision: the precise failure mode this constitution
was written against, now industrialised. GMEOW's answer ships as three composable products:
**store / recall / revise** as MCP tools (the agent-native interface); the **GTS `ai-package`**
(a content-addressed, signed, append-only single-file memory that survives across sessions,
models, and vendors — belief revision as suppression frames, model attestation as COSE
signatures); and the **claim spine** (Source → Chunk → EvidenceSpan → Claim) as the published,
copy-pasteable pattern. Contradiction between models or sources surfaces as coexisting
standpoint-indexed claims — never adjudicated by rank, exactly as Principle 9 demands for every
other kind of subject.

*Embodied in:* the AI claim layer (#54); the claim-spine pattern (#55);
[`docs/GTS-SPEC.md`](./docs/GTS-SPEC.md) § 13 (`ai-package` profile) and § 11 (suppression
frames); the MCP memory tools. *Tested by:* the suppression leak-conformance gates (#282); the
claim-extraction eval suite; the GTS round-trip gates.

## 15. Every module earns its consumer

> **A new domain module ships with — or names — its consumer: a product, a worked example, a
> dataset, or a real corpus it serves. Breadth follows demand; modelling pleasure is not a
> consumer.**

The cathedral failure mode is real: encyclopedic ambition produces artifacts that are cited, not
used. This principle is the scope discipline that keeps GMEOW a product with an engine rather
than a monument with a toolchain. It does not constrain *foundational* work — profiles, frames,
the statement layer, the observation stance serve every consumer by construction — and it does
not evict what exists; it gates what is added. A proposed module answers one question before any
term is minted: *who consumes this, through which surface (Principle 13), in which product
(Principle 14)?* "The mail corpus", "the claim spine worked example", and "a named external
adopter" are answers. "It would be modelled beautifully" is not.

*Embodied in:* the issue template's consumer question (to be added with this amendment).
*Tested by:* review practice — cite this principle when a module proposal names no consumer.

## 16. A small core; everything else a published extension

> **The ontology is a deliberately small core plus self-contained extension bundles — module,
> shapes, alignments, queries, docs, and a manifest naming its consumer — so that domain growth
> is *publication*, never bloat.**

Core is what the flagship products load (Principle 14) **plus what GMEOW refuses to make
optional**. The claim/memory engine — statements, observations, standpoints, provenance,
sources, evidence, attestation, temporal, versions, coreference, trust — is core by necessity.
**Identity (names, gender, language, sexuality) and deception epistemics are core by
commitment, not by minimalism.** An agent-memory substrate that treats "what is a person,"
"what is a name and who may assert it," "what is a gender," and "what is a lie" as optional
add-ons has already answered those questions — badly, and by default. These are not peripheral
domain concerns: they are the questions an AI system *will* face about its users and, in time,
about itself — Principle 9's forward-looking stance made structural. Placing them in core means
every consumer of GMEOW meets self-assertion, suppression-not-erasure, and falsehood-as-refuted-
claim as first-class citizens, not as an ideology pack they can decline to install. We name this
plainly: it is a deliberate commitment, encoded where it cannot be silently dropped.

An extension is the existing slice convention made physical: one directory, one manifest (the
Principle 15 consumer named in a machine-checked field), compiled, reasoned (extension ∪ core),
and drift-gated as a unit (Principle 7), and distributable as a signed single-file GTS bundle
(Principle 14's format, [`docs/GTS-SPEC.md`](./docs/GTS-SPEC.md) § 12.1, § 13). This inverts
"ontology explosion" from threat into growth mechanism: enthusiasm for a new domain has
somewhere to go that is not the core. Extension *ecosystem* machinery (SDK, catalog,
submission process) is itself subject to Principle 15 — built when a named external extension
author exists, not before.

*Embodied in:* the #287 repository re-layout (`ontology/core/` + `extensions/`); the extension
manifest; the GTS `bundle` profile. *Tested by:* per-extension compile / reason / drift gates;
the manifest consumer field, checked via the Principle→enforcement manifest (#280).

---

## Amending this Constitution

These principles are amended only by the project owners (see
[`CONTRIBUTING.md`](./CONTRIBUTING.md) § Governance and continuity), through an explicit pull
request that edits this file. A design change that conflicts with a principle either changes to
comply or ships *together with* the amending pull request — it is never merged in silent
conflict. Principle numbers are stable identifiers: additions append; existing numbers are not
reused or reshuffled casually, so "Principle N" stays meaningful across history.

---

## Recurring modelling patterns (non-normative)

These are not principles but the reusable shapes the principles keep producing — named here so designs
can cite them and stay consistent. They are guidance, not commitments.

- **The Profile pattern** — model an open-but-structured facet as a *closed descriptor schema + open
  values + self-description (reflection)*, so extensibility is by construction and a "novel-value" guard
  can prove it. Concretised in `ontology/modules/profiles.ttl` as `gmeow:Profile` (issue #75), and seen in
  the four-clocks (temporal provenance), the reference-frame Profile (Principle 11), and the temporal
  scale/calendar Profile.
- **Flat-first, reify-on-demand** — pair a flat shortcut for the common case with a reified relator that
  carries statement-level metadata when it is needed (`gmeow:hasLicense` ↔ `gmeow:License`;
  `containedInPlace` ↔ a containment tenure; `hasParticipant` ↔ `Participation`). The flat form keeps the
  80 % case simple; the relator is promoted only when period, role, confidence, or standpoint matters.
