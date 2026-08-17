<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Research-object exports

A GMEOW-described dataset, made discoverable to the ML/research ecosystem —
**generated, never hand-curated** (P4): the canonical instance data is the
single source of truth, and every format below is a lossy projection that
declares its drops (P5) in its own native slot.

Four of the five formats are projected by the Rust **purrdf 0.12.0
research-object codecs** — strict, versioned, bidirectional codecs with a
soundness-checked, located loss ledger. DCAT is deliberately kept on a
mapping-DSL CONSTRUCT (see Architecture notes).

| Format | Artifact | Producer | Consumers |
|---|---|---|---|
| **Croissant 1.1** | `<ds>.croissant.jsonld` | `purrdf::project_croissant` | Google Dataset Search, Hugging Face, Kaggle, OpenML; loadable via `mlcroissant` |
| **RO-Crate 1.3** | `ro-crate/` (metadata + preview + payloads) | `purrdf::project_ro_crate_with_assets` (Attached) | WorkflowHub, Zenodo, crate viewers |
| **DCAT 3** | `<ds>.dcat.ttl` | `dcat.rq` CONSTRUCT (mapping-DSL profile) | W3C data catalogs |
| **DataCite (kernel-4.5)** | `<ds>.datacite.xml` | `purrdf::project_datacite` (purrdf `datacite-4.6` codec; the XML declares the kernel-4.5 schema) | DOI registration |
| **Frictionless (data-package-1)** | `datapackage.json` | `purrdf::project_frictionless` | data pipelines (`frictionless`) |

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
is READ from that node, never hard-coded: the stage owns the caller vocabulary
(the `ResearchObjectRoles` binding + per-format configs) and hands it to the
codecs, which own the format-native serialization.

## The flagship worked example

`generated/research-objects/lillith/` packages the Lillith GraphRAG benchmark
(the graphrag extension's worked pipeline + the AI slice's grounded-claim
example + the eval corpus, rubric, and scores) as all five formats — the
no-drift gate (a 13-member family, pinned by its member-name set). The RO-Crate
uses **Attached** packaging: the codec emits `ro-crate-metadata.json` and a
self-contained `ro-crate-preview.html`, and the six worked-example A-Box `.ttl`
files plus the Croissant copy ride as caller-supplied `RoCrateAssets` payloads
(each `.ttl` retagged `x-gmeow`→BCP-47 and re-serialized through purrdf's
canonical Turtle). No `.crate.zip` is produced here — the crate ships as its
directory of members.

## What each export preserves / drops (P5)

Every format drops, and declares that it drops:

- **reified relators** (Copyright, roles, memberships) — flattened or absent;
- **RDF 1.2 statement annotations** — confidence, `accordingTo`, the four
  clocks;
- **standpoint indexing** — contested claims appear without their vantage
  (the `claims` recordSet keeps a `grounded` flag and `modality` tag only);
- **blake3 digests in sha256-only slots** — the verbatim `blake3:<hex>`
  survives in the Frictionless `hash`; Croissant/RO-Crate expose only the
  single `sha256` slot, so blake3/md5 are a declared drop there (a fact the
  Stage-1 round-trip test corroborates: `read_*` recovers only sha256).

Where the declaration lives: the caller authors a **"Declared drops (P5)"**
note into the dataset `description`, which every codec projects into its native
description slot (Croissant / Frictionless / RO-Crate `description`, DataCite
abstract). In addition, each codec returns a **soundness-checked structural
loss ledger** (`ensure_sound`), surfaced by the stage via
`report_projection_losses` — the machine-checked complement to the free-text
declaration.

## Architecture notes

- **`dcat` is a mapping-DSL profile** (`dsl/mappings/projections/dcat.ttl`,
  compiled to `generated/queries/dcat.rq` by `stage-mappings`): it is the ONE
  format deliberately NOT cut to a purrdf codec. Unlike the other four
  (dataset-scoped over the worked-example A-Box), the DCAT catalog is a
  CONSTRUCT over the **whole composed ontology** — every slice source — so it
  drifts with the ontology and describes the project, not just the lillith
  dataset. This is coherent under CONSTITUTION Principle 5 (SPARQL is a
  first-class by-reference alignment mechanism) and Principle 4 (an explicit,
  cache-keyed DAG branch is permitted feature selection, not degradation):
  there is one source of truth (the mapping DSL), and the mechanism split is
  justified by the genuinely different (whole-ontology) scope.
- **The other four are Rust purrdf codecs** (`project_croissant` /
  `project_datacite` / `project_frictionless` / `project_ro_crate_with_assets`).
  The former hand-rolled builders (an rdflib-parity Turtle serializer, an
  `ElementTree`-parity XML writer, and `json.dumps`-parity JSON emitters) are
  removed (greenfield). Because the codecs use their own canonical serializers
  at the blessed profile versions, this cutover is **not byte-neutral**: it
  re-blesses the committed goldens (formatting-parity is intentionally
  discarded; every remaining delta is a version bump or a declared semantic
  change).
- **RO-Crate scope change** (semantic, declared): the codec emits the canonical
  RO-Crate 1.3 metadata crate — the dataset descriptor plus its File
  distribution (the packaged payloads) with creator/publisher agents. The
  earlier hand-rolled Process/Workflow Run-Crate provenance tiers (mapping
  `gmeow:ModelInvocation` / `gmeow:BuildActivity` to `CreateAction` /
  `ComputationalWorkflow`) are not reproduced by the codec; that provenance is a
  declared drop of this cutover, tracked in the loss ledger.
- The stage is Rust end-to-end; it reads instance A-Boxes through purrdf and is
  intentionally **not** narrow-waist sealed (the seal governs exporters of the
  ontology's own data).

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
2. **WorkflowHub / Zenodo**: upload the `ro-crate/` directory; the descriptor's
   `conformsTo` rows drive profile recognition.
3. **DataCite + re3data**: deposit `<ds>.datacite.xml` under a real DOI, then
   register the hosting repository in re3data for findability.
4. **Frictionless ecosystems**: `datapackage.json` at the dataset root is
   auto-discovered by `frictionless describe`/`validate` tooling.
