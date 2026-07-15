<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Research-object exports

A GMEOW-described dataset, made discoverable to the ML/research ecosystem —
**generated, never hand-curated** (P4): the canonical instance data is the
single source of truth, and every format below is a lossy projection that
declares its drops (P5) in its own native slot.

| Format | Artifact | Consumers | Validation in-repo |
|---|---|---|---|
| **Croissant 1.0** | `<ds>.croissant.jsonld` | Google Dataset Search, Hugging Face, Kaggle, OpenML; loadable via `mlcroissant` | structural (required keys, FileObject/recordSet integrity, sha256 hex) |
| **RO-Crate 1.1** | `ro-crate/` + `<ds>.crate.zip` | WorkflowHub, Zenodo, crate viewers | structural (descriptor/root/conformsTo/hasPart, flat graph, zip integrity) |
| **DCAT 3** | `<ds>.dcat.ttl` | W3C data catalogs | the projection round-trip + dual-engine crosscheck (a mapping-DSL profile) |
| **DataCite kernel-4** | `<ds>.datacite.xml` | DOI registration | structural ElementTree pins; XSD reference-only (crossref.py stance) |
| **Frictionless** | `datapackage.json` | data pipelines (`frictionless`) | jsonschema against the vendored official Data Package profile |

Two entry points:

```bash
gmeow export all my-dataset.ttl            # arbitrary user data → dist/research-objects/
gmeow-dev sync --mode update --outputs generated research-objects          # the flagship worked example (drift-gated)
```

The input must contain a **dataset descriptor**: a `gmeow:Dataset` node with
`gmeow:title`, `gmeow:description`, `gmeow:hasLicense` (a `gmeow:License`
with `gmeow:spdxLicenseId`), `gmeow:wasAttributedTo`, and
`gmeow:datePublished` — see
`slices/extensions/graphrag/examples/lillith-dataset.ttl`. Catalog metadata
is READ from that node, never hard-coded.

## The flagship worked example

`generated/research-objects/lillith/` packages the Lillith GraphRAG benchmark
(the graphrag extension's worked pipeline + the AI slice's grounded-claim
example + the eval corpus, rubric, and scores) as all five formats, rendered
by the registered `research-objects` generator — the no-drift gate. The
`.crate.zip` is byte-deterministic (stored entries, fixed timestamps) and
lands in `dist/research-objects/` (git-ignored, published on release).

## What each export preserves / drops (P5)

Every format drops, and declares that it drops:

- **reified relators** (Copyright, roles, memberships) — flattened or absent;
- **RDF 1.2 statement annotations** — confidence, `accordingTo`, the four
  clocks;
- **standpoint indexing** — contested claims appear without their vantage
  (the `claims` recordSet keeps a `grounded` flag and `modality` tag only);
- **blake3 digests in sha256-only slots** — Croissant's `cr:sha256` is left
  empty for blake3 sources; the verbatim `blake3:<hex>` digest survives in
  the Frictionless `hash`, the RO-Crate File `identifier`, the DCAT
  `spdx:checksumValue`, and a Croissant `description` note.

Where the declaration lives: Croissant `rai:dataLimitation` · RO-Crate
descriptor `description` · Frictionless `notes` · DataCite
`<description descriptionType="TechnicalInfo">` · the DCAT profile's
`gmeow:lossyDrop` cells (compiled into the query header).

## Architecture notes

- **`dcat` is a mapping-DSL profile** (`dsl/mappings/projections/dcat.ttl`):
  RDF→RDF belongs to the compiler (EDOAL + FnO + CONSTRUCT + `gmeow project
  dcat` for free).
- **The other four are Python builders**
  (`src/gmeow_tools/research_objects.py`): their document shapes (Croissant's
  layered JSON-LD, RO-Crate's flat `@graph`, plain-JSON Frictionless, DataCite
  XML) need framing a CONSTRUCT cannot express — a DSL layer would be a dead
  declarative twin. This deviation from the original acceptance wording is
  deliberate and authorized by its own "pure-rdflib + hand-rolled packager"
  allowance.
- **Tiered Run-Crate conformance, honestly earned (P1)**: every crate is at
  least a **Process Run Crate** — `gmeow:ModelInvocation` /
  `gmeow:ImportActivity` map to `CreateAction` (instrument = the
  `SoftwareAgent`, objects from `wasDerivedFrom`, results from
  `wasGeneratedBy`). When the A-Box carries a **workflow run** — a
  `gmeow:BuildActivity` whose `gmeow:buildConfigUri` names the workflow
  definition — the crate upgrades to **Workflow Run Crate**: the definition
  becomes the `ComputationalWorkflow` `mainEntity`, the run's `CreateAction`
  takes it as `instrument`, the `gmeow:Builder` participant becomes the
  `agent`, and `buildSource`/`buildOutput` become `object`/`result`. The
  flagship example exercises the full tier.
- The module reads instance A-Boxes with rdflib and is intentionally **not**
  narrow-waist sealed (the seal governs exporters of the ontology's own
  data).

## External validation recipes (before publishing)

```bash
pip install mlcroissant && \
  python -c "import mlcroissant as mlc; mlc.Dataset(jsonld='lillith.croissant.jsonld')"

pipx run rocrate-validator validate ro-crate/        # RO-Crate profile check

xmllint --noout --schema \
  https://schema.datacite.org/meta/kernel-4.5/metadata.xsd lillith.datacite.xml

pipx run frictionless validate datapackage.json
```

(The optional `mlcroissant` test runs automatically when the package is
importable; it is never a build dependency.)

## Discovery recipe

1. **Google Dataset Search**: serve the Croissant JSON-LD at (or linked from)
   the dataset's landing page (`sc:url`); the crawler indexes
   schema.org/Croissant markup.
2. **WorkflowHub / Zenodo**: upload the `.crate.zip`; the descriptor's
   `conformsTo` rows drive profile recognition.
3. **DataCite + re3data**: deposit `<ds>.datacite.xml` under a real DOI (the
   committed artifact carries the reserved 10.5072 TEST prefix until the
   actual publish act) and register the hosting repository in re3data for
   findability.
4. **Frictionless ecosystems**: `datapackage.json` at the dataset root is
   auto-discovered by `frictionless describe`/`validate` tooling.
