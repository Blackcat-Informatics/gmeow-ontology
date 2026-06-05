<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Why GMEOW exists

> GMEOW — the Global Metadata and Entity Ontology for the Web — is a
> reasoning-centric, OWL 2 DL, gUFO-grounded **super-vocabulary** for modelling a
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
~20 vocabularies — the modelling problem resolves into seven concrete challenges:

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

## The solution

GMEOW answers each challenge with a deliberate architectural choice:

- **A canonical superset, not yet-another-vocabulary.** GMEOW mints one canonical
  term per concept (`gmeow:Person`, `gmeow:OnlineAccount`, `gmeow:SoftwareProject`…)
  and **aligns** it to every surface vocabulary via `owl:equivalentClass` /
  `rdfs:subClassOf` / `owl:equivalentProperty`. Data already published in FOAF,
  schema.org, GEDCOM, vCard, DOAP, … is **covered by reference** — you never rewrite
  it. *(Addresses fragmentation and breadth.)*
- **A foundational (gUFO) spine.** Every GMEOW class is grounded under a gUFO
  category (Endurant, Event, Relator, Object, Role). This is what lets a 20-vocabulary
  union stay *coherent and reasonable* rather than collapse into contradiction.
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
- **Reasoning-centric and FAIR-published.** OWL 2 DL, checked by ELK (fast) and
  HermiT (sound + complete) on every build; published with content negotiation,
  VoID/DCAT, a DOI, and submitted to the LOD Cloud.

## How it grows: slices

GMEOW is built **incrementally, one slice of digital existence at a time**. Each
slice adds canonical terms in a module, alignment tables to that domain's surface
vocabularies, and a vendored fixture; the `coverage` tool then measures exactly how
much of the slice GMEOW covers and lists the remaining gaps. The first slice is
**entities + contacts**; planned next are **email → documents → temporal events →
calendar → notes → projects → …**.

Because coverage is measured against real data, "have we modelled digital existence
yet?" stops being a vibe and becomes a number with an explicit, shrinking gap list.

## See also

- [`CONSTITUTION.md`](../CONSTITUTION.md) — the ten normative principles these choices answer to.
- [`README.md`](../README.md) — the toolchain and how to build/validate/publish.
- [`LICENSING.md`](../LICENSING.md) — dual licensing (Apache-2.0 tooling / CC BY 4.0 vocabulary).
- `mappings/` — the SSSOM alignment tables that make the superset real.
- `tests/fixtures/coverage/` — the public data slices coverage is measured against.
