<p align="center">
  <img src="docs/gmeow-logo.svg" alt="GMEOW logo" width="160" height="160">
</p>

# GMEOW - Global Metadata and Entity Ontology for the Web

> **An LLM output is a claim, not a truth.**

GMEOW is an RDF 1.2-first, OWL 2 DL ontology for describing digital existence:
people, organizations, documents, sources, claims, observations, events, places,
rights, names, identity facets, software, evidence, trust, and the standpoints
from which those things are asserted.

The central idea is simple: durable metadata should not store disputed or
machine-generated assertions as flat truth. A fact in GMEOW is an attributed,
time-scoped, confidence-weighted, evidence-linked claim. Contradictory claims
coexist. Superseded claims are suppressed rather than deleted. Names, identities,
locations, measurements, rights, and provenance are all modeled as first-class
relationships instead of strings in a single slot.

GMEOW is a super-ontology: it keeps a rich canonical model and links outward to
consumer vocabularies. It can project to simpler surfaces such as schema.org,
FOAF, vCard, GeoSPARQL, iCalendar, OWL-Time, ODRL, Dublin Core, SPDX, RDF Data
Cube, Web Annotation, and OntoLex-Lemon without making those flattened views the
source of truth.

## What GMEOW Is For

- **Grounded AI memory.** Model LLM and agent outputs as claims with source,
  evidence, time, confidence, and revision history.
- **Contested knowledge.** Represent incompatible claims without choosing a
  global winner: each assertion can be indexed by standpoint, source, confidence,
  and temporal scope.
- **Identity-safe metadata.** Model names, pronouns, honorifics, gender,
  sexuality, languages, and display suppression without primary/preferred
  shortcuts that erase context.
- **Source-aware documents and evidence.** Keep carrier provenance, source
  provenance, claim provenance, citation acts, import lineage, and evidence
  warrants separate.
- **Frame-relative measurement.** Attach coordinates, quantities, observations,
  prices, dates, colors, and locations to explicit reference frames.
- **Rights and policy.** Describe licenses, copyright, trademarks, permissions,
  prohibitions, duties, constraints, parties, and temporal validity as structured
  rights claims.
- **Interoperability.** Publish and consume familiar vocabulary surfaces while
  preserving the richer GMEOW graph for applications that need provenance,
  disagreement, suppression, and reasoning.

## Core Commitments

GMEOW's modeling commitments are written in
[`CONSTITUTION.md`](./CONSTITUTION.md). The most important ones for ontology
readers are:

- **Claim, not truth.** Assertions are statements from a vantage, not privileged
  global facts.
- **RDF 1.2-first.** Statement-level metadata is native to the model. OWL axiom
  annotations are the compatibility form for current OWL tooling.
- **One canonical source.** Rich GMEOW terms are the source. Consumer schemas are
  projections.
- **Maximal linking.** External vocabularies are linked by reference whenever
  possible, without copying incompatible axioms into the reasoning closure.
- **No privileged winner.** Coexisting names, identities, standpoints, and claims
  are not collapsed to a single primary value.
- **Suppression, never erasure.** Data that should no longer display remains
  auditable with `gmeow:displayable false`.
- **Frame relativity.** Measurements and locations are meaningful only inside an
  explicit reference frame.
- **Solver boundary.** Heavy planning, conversion, matching, and optimization
  belong in solver/tooling layers, not asserted triples.

## Ontology Shape

The ontology is organized as **slices**. Each slice owns a coherent vocabulary
area: its classes, properties, shapes, examples, mappings, queries, and prose
guidance. Core slices compose into the core ontology; extension slices add
domain-specific coverage while depending on core concepts.

Representative core areas include:

| Area | What it covers |
|---|---|
| Kernel and logic | Foundational entities, reference frames, statement semantics, constraints, and conformance doctrine |
| Provenance, sources, evidence, citations | Where claims came from, how sources were imported, what evidence supports them, and how citation acts are represented |
| Standpoint and trust | Vantage-indexed claims, confidence, belief, denial, trust assertions, and coexisting disagreement |
| Names, gender, sexuality, language | Co-equal identity facets, contextual naming, language/script modeling, pronouns, honorifics, and display suppression |
| Documents, creative works, rights | Works, expressions, manifestations, document metadata, licenses, copyright, trademarks, permissions, prohibitions, and duties |
| Time, events, places, observations | Temporal intervals, events, locations, trajectories, measurements, quantities, procedures, observations, and data quality |

Representative extension areas include email, software, finance, employment,
genealogy, images, music, notes, accessibility, archaeology, connectivity,
narrative, norms, notation, risk, sensory environments, and graph/RAG patterns.

The formal logic documentation is slice-owned under
[`slices/core/logic/design/`](./slices/core/logic/design/). Start with:

- [`LOGIC.md`](./slices/core/logic/design/LOGIC.md) - the overall logic doctrine;
- [`LOGIC-SEMANTICS.md`](./slices/core/logic/design/LOGIC-SEMANTICS.md) - the
  statement, standpoint, negation, and entailment semantics;
- [`LOGIC-CONFORMANCE.md`](./slices/core/logic/design/LOGIC-CONFORMANCE.md) -
  conformance expectations for tools and profiles;
- [`LOGIC-RUNTIME.md`](./slices/core/logic/design/LOGIC-RUNTIME.md) - runtime
  behavior for reasoning and projection surfaces;
- [`LOGIC-REFERENCES.md`](./slices/core/logic/design/LOGIC-REFERENCES.md) -
  cited logic and ontology references.

GMEOW currently uses gUFO as its imported upper-ontology spine. Consumers should
read GMEOW terms first; gUFO categories such as `gufo:IntrinsicMode` are formal
grounding vocabulary, not the user-facing API.

## RDF 1.2 Statements

GMEOW treats statement metadata as first-class RDF 1.2/RDF-star content. The same
assertion can carry:

- who asserted or recorded it;
- which standpoint it belongs to;
- when it was valid;
- when it was asserted;
- confidence and quality metadata;
- evidence and source links;
- suppression or supersession state.

Current OWL 2 DL reasoners do not reason directly over RDF 1.2 triple terms, so
GMEOW also maintains an OWL axiom-annotation compatibility view. That OWL view is
not a second source of truth; it is the reasoning surface for today’s tooling.

## Interoperability

GMEOW links outward in two ways:

- **Projection** creates a simpler target graph for a consumer vocabulary. This
  is deliberately lossy and documented as such.
- **Alignment by reference** records relationships to external concepts without
  importing their axioms into GMEOW.

Common projection targets include:

| Target | What GMEOW can emit |
|---|---|
| schema.org | Flat person, organization, place, claim, work, and metadata surfaces |
| FOAF | Basic agent, name, homepage, mailbox, and social graph facts |
| vCard RDF | Contact-card facts, including projected pronouns |
| GeoSPARQL | Geometries and spatial/topological relations |
| iCalendar RDF and OWL-Time | Event and interval views |
| ODRL and Creative Commons REL | Rights, permissions, prohibitions, duties, and licenses |
| Dublin Core and SPDX | Bibliographic, rights, and software-license metadata |
| RDF Data Cube | Statistical observation/data-set projections |
| Web Annotation | Annotation, tag, and standpoint surfaces |
| OntoLex-Lemon | Lexical entries and written forms from names/language data |

Representative alignment families include PROV-O, ORG, Wikidata, BFO, DOLCE,
SUMO, CIDOC CRM, CRMinf, QUDT, SOSA/SSN, SensorThings, FALDO, Sequence Ontology,
IVOA, SWEET, FIBO, Homosaurus, GSSO, FHIR, PREMIS, RightsStatements.org, DPV,
SLSA, DSSE, Sigstore/Rekor, SCITT, nanopublications, and many domain standards.

See [`docs/projections.md`](./docs/projections.md),
[`docs/wikidata-mapping.md`](./docs/wikidata-mapping.md), and
[`docs/foundational-bridging.md`](./docs/foundational-bridging.md) for the
details.

## The Portable Ontology Bundle

The usable ontology surface is shipped as **`gmeow.gts`**, a Graph Transport
Substrate bundle. It folds the ontology graph, useful imports, mappings,
projection queries, statement views, examples, and documentation blobs into one
portable artifact.

That matters because a consumer should not need a source checkout, external
reasoners, generator inputs, or local query trees to use the ontology. The
`gmeow` CLI reads the bundled `gmeow.gts` snapshot and exposes the ontology
directly:

```bash
pip install gmeow

gmeow info
gmeow describe gmeow:Observation
gmeow docs --directory gmeow-docs
gmeow export --out gmeow-export
```

Signed publication bundles can be checked with `gmeow verify path/to/gmeow.gts`;
see [`docs/VERIFY-EXAMPLE.md`](./docs/VERIFY-EXAMPLE.md).

The same bundle can be used as input:

```bash
gmeow project --profile foaf --out bundled-foaf-view
gmeow project my-data.ttl --profile foaf --out my-foaf-view
gmeow transpile source.ttl --out transpiled
```

Use [`docs/GTS-SPEC.md`](./docs/GTS-SPEC.md) for the transport format.

## Documentation

The most useful documentation is generated from the ontology itself: slice
metadata, term annotations, examples, mappings, citations, and slice-local design
docs. For an offline copy:

```bash
gmeow docs --directory gmeow-docs
```

Authoritative source docs include:

| Document | Purpose |
|---|---|
| [`CONSTITUTION.md`](./CONSTITUTION.md) | Normative ontology principles |
| [`docs/RATIONALE.md`](./docs/RATIONALE.md) | Why GMEOW exists |
| [`docs/CITATIONS.md`](./docs/CITATIONS.md) | Citation and reference policy |
| [`docs/GTS-SPEC.md`](./docs/GTS-SPEC.md) | GTS transport format |
| [`docs/VERIFY-EXAMPLE.md`](./docs/VERIFY-EXAMPLE.md) | Signature and bundle verification example |
| [`docs/projections.md`](./docs/projections.md) | Projection and mapping doctrine |
| [`docs/transpile.md`](./docs/transpile.md) | Consumer RDF to GMEOW to multi-vocabulary transformation |
| [`docs/standpoints.md`](./docs/standpoints.md) | Standpoint-indexed contested facts |
| [`docs/rights.md`](./docs/rights.md) | Rights and IP modeling |
| [`docs/location-mapping.md`](./docs/location-mapping.md) | Place, location, frame, and spatial modeling |
| [`docs/identity-mapping.md`](./docs/identity-mapping.md) | Names, gender, sexuality, and identity facets |
| [`docs/music-mapping.md`](./docs/music-mapping.md) | Music ontology guide |
| [`slices/core/logic/design/LOGIC.md`](./slices/core/logic/design/LOGIC.md) | Logic doctrine and semantics entry point |

Slice-local `docs.md`, `examples/`, `mappings/`, `queries/`, and `design/`
directories are part of the ontology documentation, not incidental repository
notes.

## Key Modeling Areas

### Claims, Standpoints, And Evidence

GMEOW keeps `gmeow:accordingTo`, `gmeow:wasAttributedTo`, evidence, confidence,
and temporal validity separate. A source can record someone else's standpoint; a
claim can be denied by one standpoint and accepted by another; neither assertion
becomes the global winner.

### Names And Identity

Names are reified appellations and usages. A person can have co-equal names in
multiple scripts, contexts, and time periods. Pronouns and honorifics are
contextual facets independent of gender. Former names and former identity labels
can remain in the graph while being suppressed from display.

### Places, Frames, And Measurements

`gmeow:Location` is not just latitude and longitude. It is an entity located
with respect to a reference frame. The same kernel covers terrestrial,
celestial, indoor, virtual, robotic, mathematical, biological-sequence,
fictional, and cognitive spaces. Measurements carry units, uncertainty, and
frames.

### Rights

Rights are instance-level claims over works, data, software, marks, people, and
organizations. Licenses are agreements. Permissions, prohibitions, duties,
constraints, remedies, holders, parties, and validity windows are structured
rather than hidden in strings.

### Observations And Scientific Data

Observations are claims from a vantage. GMEOW aligns to SOSA/SSN, SensorThings,
QUDT, DQV, ISO 19157, FALDO, Sequence Ontology, IVOA, SWEET, and related
scientific vocabularies while keeping frame, unit, uncertainty, source, and
standpoint explicit.

## Identity And Publication

- **Canonical IRI:** <https://blackcatinformatics.ca/gmeow>
- **Term IRI shape:** `https://blackcatinformatics.ca/gmeow/<LocalName>`
- **Vocabulary license:** [CC BY 4.0](./LICENSE-ontology)
- **Tooling license:** [Apache-2.0](./LICENSE)
- **Copyright:** © 2026 Blackcat Informatics® Inc.

GMEOW is dual-licensed: the ontology vocabulary is licensed under CC BY 4.0, and
the tooling code is licensed under Apache-2.0. Full terms are in
[`LICENSING.md`](./LICENSING.md); attribution and trademark notices are in
[`NOTICE`](./NOTICE).
