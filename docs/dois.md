<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# DOIs in GMEOW — the single-anchor strategy

GMEOW registers **one** Crossref DOI today — the **concept DOI**
`10.67342/26w4o` — with room for **one optional version DOI per release** and,
later, **per-profile sub-DOIs**. This is a deliberate design choice, not a
limitation: a Crossref prefix permits unlimited minting, but minting a DOI per
module, per mapping-set, or per statement reproduces the field's recurring DOI
failures rather than fixing them. Instead the DOI is a single stable *citation
anchor* that points **into** a content-addressed graph, and the graph carries the
granularity, versioning, and provenance.

## The identifier triangle

Three identifier kinds answer three different questions. GMEOW keeps them
distinct and bridges them, rather than overloading one DOI to do all three:

| Leg | Identifier | Answers | Where it lives |
| --- | --- | --- | --- |
| **Extrinsic / citation** | the DOI | "how do I cite this?" | `metadata/gmeow-self.ttl`, `CITATION.cff` |
| **Semantic** | `owl:versionIRI` / term IRIs | "what does this mean?" | the ontology header + namespace |
| **Intrinsic / exact bytes** | SWHID, `gmeow:gtsHeadId`, `gmeow:contentDigest` (blake3) | "are these the exact bytes I reasoned over?" | the git Merkle DAG + the GTS package |

The work the dropped per-version / per-component / per-statement DOIs would have
done is done by the **intrinsic leg**, which GMEOW gets for free: the repository
is a content-addressed Merkle DAG, and every release ships a GTS package whose
chained **head id** is the intrinsic identifier of exactly those bytes.

## What the DOIs denote (FRBR-aligned)

The self-description already models the work as a WEMI spine, so the DOI layer
*projects* that structure — it invents no new shape (Principle 4):

- **Concept DOI → the Work** (`https://blackcatinformatics.ca/gmeow`). The
  always-latest citation anchor; resolves to the concept IRI. This is the single
  registered DOI. (`gmeow:VersionSet` literally names "the DOI concept record" as
  an example — the lineage is the citable thing.)
- **Version DOI → the Manifestation** (`…/gmeow/<semver>`, the
  `owl:versionIRI`). Optional; one per immutable release. Answers "*which version
  did you reason over?*". **Not yet minted — concept-only is a first-class state.**
- **Profile sub-DOIs → deferred.** When demand exists, a profile (e.g.
  `claims` / `memory` / `narrative`) can earn a Crossref `<component>` under the
  version DOI (`parent_relation="isPartOf"`), keyed off the profile's
  content-addressed identity. The deposit reserves the seam (a documented comment
  where the `<component_list>` would go); populating it is additive, never
  structural.

The concept↔version edge is read by **role** (Work DOI vs Manifestation DOI),
following the existing `realizes` / `embodies` WEMI chain — never inferred from
position in the file.

## The Crossref deposit (generated on demand, hand-submitted)

The deposit is a **transient submission document**, not a published artifact:
there is no automated upload. `gmeow-dev crossref` generates
`dist/crossref-deposit.xml` (ephemeral build output, never committed); the
registrant **hand-verifies it and submits it to Crossref manually**. Because each
submission needs a live, monotonically-increasing `<timestamp>` (Crossref uses it
to order resubmissions), the deposit is generated fresh each time — it is
deliberately *not* a committed, drift-gated artifact.

`src/gmeow_tools/crossref.py` builds it from the self-description (Principle 4:
generated from the one canonical source) and **uses the whole schema** — the
output validates against `crossref5.4.0.xsd` + `AccessIndicators.xsd` +
`relations.xsd`. Beyond `<dataset type="record">` it carries:

- **`<contributors>`** — the organization (Blackcat Informatics® Inc.) **and**
  ORCID-identified persons, projected from the `gmeow:Contribution` credit graph
  (author role, organizations first).
- **`<description>`, `<format>`, dual `<database_date>`** (publication + update),
  **`<version_info>`**, and **`<publisher_item>`** identifiers.
- **`<publisher>`** (name + place) and **`<institution>`** (name + acronym + place).
- **`<institution_id type="wikidata">`** for the Blackcat Informatics® Inc.
  organization QID, projected from the self-description authority link.
  Patrick Audley's Wikidata QID is also canonical self-description metadata, but
  Crossref's native person-contributor surface is ORCID rather than a generic
  Wikidata person PID field; the deposit therefore emits his ORCID and does not
  spoof the personal QID into a non-native Crossref element.
- **`ai:program` (AccessIndicators)** — `free_to_read` + `license_ref` for the CC
  license, making the license machine-readable in the PID graph for both the
  version of record (`applies_to="vor"`) and text/data-mining use
  (`applies_to="tdm"`).
- **Text-mining full-text URLs** — Crossref
  `<collection property="text-mining">` resources under `<doi_data>` for the
  public machine-readable serializations: Turtle, RDF/XML, N-Triples, JSON-LD,
  and the GTS package. The public HTTP/signposting media type for GTS is
  `application/vnd.blackcat.gts+cbor-seq`; Crossref 5.4.0's media-type enum does
  not accept that vendor subtype, so the Crossref deposit uses the schema-valid
  parent type `application/cbor-seq` for the `.gts` TDM URL.
- **`<intra_work_relation>`** — `hasFormat` to every published serialization (the
  Crossref-native analog of the Signposting `item` links), plus `hasVersion` /
  `isVersionOf` binding the two DOIs when a version DOI exists.
- **`<inter_work_relation>`** — `isSupplementedBy` → the source repository, and the
  curated `ALIGNMENT_TARGETS` registry projected to `isDerivedFrom` (upper
  ontologies) / `references` (peer schemas), identified by DOI when known else
  namespace URI. Crossref auto-creates the reverse link, making the deposit a
  first-class PID-graph node.
- **`<citation_list>`** — explicit Crossref references for the same curated
  alignment targets, so reference metadata is available through Crossref's
  reference channels rather than only through relation metadata.
- **`<component_list>` seam** — a documented comment marking where future
  per-profile sub-DOIs attach.

The current Crossref 5.4.0 `database` / `dataset` record model does not expose a
schema-valid generic subject or keyword element. GMEOW's descriptive subject
set for this DOI is: ontology; semantic web; linked data; knowledge
representation; RDF; OWL; SHACL; FAIR data; metadata; persistent identifiers;
agent memory. Those subjects belong in GMEOW's own RDF/catalog projections and
documentation until Crossref documents a dataset-valid field for them; the
deposit must not spoof unrelated fields just to raise a participation-report
percentage.

Likewise, the deposit intentionally omits Similarity Check URLs, ROR IDs, funder
registry/award metadata, and Crossmark metadata until those facts or service
enrollments exist.

## FAIR Signposting bridge

`src/gmeow_tools/apache.py` emits typed `Link:` headers on the `/gmeow` landing
page (sourced from the self-description, so the DOI is never hard-coded):

- `rel="cite-as"` → the DOI (citation identity);
- `rel="describedby"; type="text/turtle"` (+ rdf/nt/jsonld) → the RDF (semantic);
- `rel="item"` → every serialization **including the `.gts` package** whose head
  id is the intrinsic identity.

This closes the loop machine-actionably: a crawler landing on the page learns the
persistent identifier, where the RDF is, and the exact bytes — all three legs.

## doi-lint (runs at generation time)

`lint_deposit()` runs **before** the deposit is written — `gmeow-dev crossref`
refuses to emit a deposit that fails it, so an inconsistent submission document
can never be produced. It asserts **format / consistency only** (never a network
resolve — our own DOI is undeposited until submitted, so it 404s):

1. no `10.XXXXX` placeholder survives in the self-description, `CITATION.cff`, or
   the deposit;
2. the deposit's DOIs equal the self-description's DOIs;
3. the concept resource is the concept IRI and the version resource is the
   `owl:versionIRI`; term IRIs carry no version;
4. concept-only (no version DOI) is valid and not flagged.

The same invariants are pinned by `tests/test_crossref.py`.

## What hasn't worked elsewhere (and why this fixes it)

- **One DOI tracking a mutable target.** Citing a PURL or a whole-ontology DOI
  that silently follows "latest" makes "which version did you reason over?"
  unanswerable. Here the concept DOI is *explicitly* always-latest, and the
  version IRI + intrinsic head id name an exact, immutable artifact.
- **The two/three-identifier gap.** A DOI usually resolves to an HTML page, never
  to `text/turtle` or to verifiable bytes. Signposting + the GTS head id bridge
  all three.
- **Granularity by minting.** Minting a DOI per module/statement creates a
  registry to maintain and rot. Granularity here is content-addressed and free.

## Minting the version DOI (operational note)

The concept DOI is live. To add the version DOI for a release: mint a second
Crossref DOI, add it as `dcterms:identifier` on the Manifestation in
`metadata/gmeow-self.ttl`, regenerate, and deposit. The generator then emits the
two-record relational deposit automatically; no code change is needed.
