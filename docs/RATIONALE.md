<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Why GMEOW exists

> GMEOW — the Global Metadata and Entity Ontology for the Web — is a
> reasoning-centric, RDF 1.2-native, grounding-slice-founded **super-vocabulary** for modelling a
> person's or organization's *digital existence*.

This document explains the **problems** GMEOW answers; [`CONSTITUTION.md`](../CONSTITUTION.md)
states the **commitments** the answers must honour. They complement, not duplicate, each other.

## The reason

People and organizations now exist, in large part, as data: contacts, email,
calendar, documents, notes, projects, accounts, publications, genealogy, places,
agreements, social presence. That record is spread across dozens of services and
file formats, and — crucially — across dozens of **overlapping, incompatible RDF
vocabularies**. There is no single vocabulary that spans a whole digital life, and
the ones that exist describe the *same facts in different, non-interoperable ways*.

GMEOW exists to be the **unifying, reasoning-capable layer** over that sprawl: one
coherent set of canonical terms, grounded in a foundational ontology, that every
surface vocabulary aligns to — so the union of someone's data becomes a single,
queryable, inference-friendly graph instead of a pile of disconnected silos.

## The problems

Looking at a single real person's public linked-data — which already touches
~20 vocabularies — the modelling problem resolves into nine concrete challenges:

1. **Vocabulary fragmentation (N ways to say one thing).** A person is
   `foaf:Person` *and* `schema:Person`; kinship is expressed in GEDCOM *and* REL
   *and* schema.org; an email is `schema:email` *and* `vcard:hasEmail` *and*
   `foaf:mbox`. No vocabulary is authoritative and they overlap inconsistently.
2. **Domain-silo breadth.** Digital existence spans contacts, email, calendar,
   documents, notes, projects, genealogy, accounts, finance, agreements… and each
   life-domain is its own mature-but-isolated vocabulary.
3. **Coreference.** The same person, place, or concept recurs across every source
   under different identifiers, demanding `owl:sameAs` / `skos:exactMatch` links to
   shared authorities (Wikidata, GeoNames, Getty, ESCO, ORCID).
4. **Provenance and confidence.** Facts arrive from many sources of varying trust;
   each *statement* needs attribution and a confidence weight.
5. **Temporal validity.** Almost nothing is timeless — jobs, addresses,
   relationships, "has met", accounts hold over intervals.
6. **Lossless capture vs. a clean canonical model.** You must preserve every
   source field *and* expose a tidy, queryable canonical layer.
7. **Reasoning over the union.** To *use* the unified graph — to infer that three
   vocabularies' "person" terms denote one thing — you need a coherent ontology and
   a reasoner, not just a bag of triples.
8. **Contested facts (no single truth).** History, geopolitics, genealogy, policy,
   research claims, and AI-generated claims are *disputed* — the same fact carries rival
   values that different parties each hold. A flat model gives it one slot two parties
   must fight over.
9. **Self-determination and display safety.** Identity, naming, gender and orientation are
   self-asserted, change over time, and carry real-world risk if mishandled — a deadname
   must never leak, and "primary"/"preferred" privileging is itself a harm.

## The solution

GMEOW answers each challenge with a deliberate architectural choice:

- **A canonical superset, not yet-another-vocabulary.** GMEOW mints one canonical
  term per concept (`gmeow:Person`, `gmeow:OnlineAccount`, `gmeow:SoftwareProject`…)
  and **aligns** it to every surface vocabulary via `owl:equivalentClass` /
  `rdfs:subClassOf` / `owl:equivalentProperty`. Data already published in FOAF,
  schema.org, GEDCOM, vCard, DOAP, … is **covered by reference** — you never rewrite
  it. *(Addresses fragmentation and breadth.)*
- **A co-foundational grounding spine.** `logic:` owns the foundational sorts and
  relations, `lang:` owns meaning and form, and `math:` owns formal and quantitative
  structure. Domain slices consume that triad rather than grounding themselves in an
  external vocabulary. gUFO and OWL are generated target views; BFO, OBO, and SUMO are
  by-reference commitment-shifting bridges, never competing sources of meaning. This is
  what lets a 20-vocabulary union stay *coherent and reasonable* rather than collapse into contradiction.
  *(Addresses reasoning over the union.)*
- **Coreference by alignment.** Canonical IRIs plus `skos:exactMatch`/`owl:sameAs`
  links to external authorities make coreference a first-class, queryable seam.
  *(Addresses coreference.)*
- **Provenance, confidence and time as the canonical RDF 1.2 / RDF\* layer over an
  OWL-DL core** ([Principle 2](../CONSTITUTION.md)). Statement-level `gmeow:confidence`,
  `gmeow:importanceLevel`, `gmeow:mappedFrom` and temporally-scoped relationships are
  authored as native RDF 1.2 metadata and annotate claims without disturbing the decidable
  logical core; the OWL axiom-annotation form a reasoner consumes is a *generated downcast*
  of this layer ([Principle 3](../CONSTITUTION.md)). *(Addresses provenance/confidence,
  temporal validity, and lossless-vs-canonical.)*
- **Reasoning-centric and FAIR-published.** OWL 2 DL, checked by the native `logic:`
  reasoner (fast EL pre-check plus a sound-and-complete OWL 2 DL check, cross-checked
  in-process against the `purrdf::entail` oracle) on every build; published with content negotiation,
  VoID/DCAT, a DOI, and submitted to the LOD Cloud.
- **Contested facts as coexisting standpoints — no winner.** A disputed fact is recorded
  as several `gmeow:accordingTo`-indexed claims that coexist, none privileged — *whose
  frame* (the standpoint) held apart from *which source* recorded it and *how sure* we are.
  There is no `preferredRank`/`primary*` — refused by a SHACL shape, a statement-DSL lint,
  and a term-absence test — so the reasoned graph stays consistent while the disagreement is
  preserved. *(Addresses contested facts.)* See [`standpoints.md`](./standpoints.md).
- **Reified, self-asserted identity with display safety.** Names and identity facets are
  co-equal and self-asserted — no `primaryName`/`preferredGender`; orthogonal axes
  (pronouns, honorifics, gender identity/expression, sex, sexual/romantic orientation) are
  *test-enforced* not to infer one another; a superseded label (a deadname, a former
  gender) is kept with `gmeow:displayable false` — retained, **never displayed, never
  deleted**. *(Addresses self-determination and display safety.)*
- **Frame-relativity — values carry their reference system.** A coordinate, date, price or
  name is meaningless without its frame (a CRS, a calendar + timescale, a currency, a
  register); GMEOW makes the frame explicit and first-class, separating frame-independent
  *structure* from frame-relative *value* ([Principle 11](../CONSTITUTION.md)), with heavy
  conversion computed in an external solver, never asserted ([Principle 12](../CONSTITUTION.md)).
  *(Addresses lossless capture and cross-system coreference; now spans 13+ realms from
  terrestrial to celestial to biological-sequence to fictional.)*
- **Observation & measurement as first-class claims.** Built on frame-relativity, a universal
  `gmeow:Observation` (SOSA/SensorThings) and `gmeow:Quantity`/`MeasuredValue` (QUDT) make
  every measurement an attributed, unit-bearing, frame-aware claim — with ontic *determinacy*
  held apart from epistemic *confidence*, and data quality recorded against W3C DQV / ISO 19157.
  This is what turns GMEOW from a person-metadata vocabulary into one that scientific
  data — astronomy, genomics, robotics, n-D mathematics — can use directly. *(Addresses
  provenance/confidence and reasoning over the union, for measured data.)*

## How it grows: slices

GMEOW is built **incrementally, one slice of digital existence at a time**. Each
slice adds canonical terms in a module, alignment tables to that domain's surface
vocabularies, projections, and a vendored fixture; the `coverage` tool then measures
exactly how much of the slice GMEOW covers and lists the remaining gaps.

**Built so far** — the identity, naming, language, gender/sexuality, contact, email,
account, genealogy, organization, document, source, software, expertise, agreement, rights,
trust, tags, versions, lifecycle, connectivity, and coreference modules; place, temporal, and
event; the **unified epistemics & measurement spine** (provenance, standpoint, and
observation — where a standpoint-indexed claim *is* an observation-from-a-vantage); the
Location universal reference-frame epic (13+ realms: terrestrial, indoor, virtual/network,
celestial, mathematical/n-D, psychological, robotic, fictional, biological-sequence,
geocoding, cadastral, maritime/aviation, sensory); cross-cutting foundations
(frame-relativity, determinacy, granularity, privacy, accessibility, spatial aggregation,
regulatory overlays, data quality, attestation); and the reasoning spine (axiomatized
doctrine, the OWL+SHACL split, the gUFO↔BFO foundational bridge). Recent epics have markedly
expanded GMEOW's **scientific utility** — QUDT-aligned quantities and frame-relative
observation across astronomy, genomics, robotics, and n-D mathematics.
**Planned, tracked as issues** — deeper Languages (diachronic / sociolinguistic / symbolic /
archaeological), complete email coverage, scientific Observation profiles (archaeology,
astronomy, clinical, media), and new slices (finance, calendar, notes, employment, images,
books, projects/software); an AI / RAG claim-provenance layer (claim-not-truth,
evidence-bound claims); and broad-consumption tooling (LinkML developer schemas,
property-graph and ML-dataset / research-object exports, a maximal DOI strategy, an MCP
server). The current themes, with issue numbers, are in the
[README roadmap](../README.md#roadmap).

Because coverage is measured against real data, "have we modelled digital existence
yet?" stops being a vibe and becomes a number with an explicit, shrinking gap list.

## See also

- [`CONSTITUTION.md`](../CONSTITUTION.md) — the twelve normative principles these choices answer to.
- [`README.md`](../README.md) — the toolchain and how to build/validate/publish.
- [`LICENSING.md`](../LICENSING.md) — dual licensing (AGPL-3.0-only tooling / CC BY 4.0 vocabulary).
- `mappings/` — the SSSOM alignment tables that make the superset real.
- `tests/fixtures/coverage/` — the public data slices coverage is measured against.
