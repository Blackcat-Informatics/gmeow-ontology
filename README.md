# GMEOW — Global Metadata and Entity Ontology for the Web

GMEOW is a **reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary**
that unifies document metadata, entity descriptions, legal agreements, contacts and
person-centric data — a "super" FOAF + REL + DOAP + GEDCOM + PROV-O. It is grounded in
**gUFO** and aligned to FOAF, REL, DOAP, PROV-O, ORG, schema.org and **Wikidata**.

- **Canonical IRI:** <https://blackcatinformatics.ca/gmeow> (slash namespace, term IRIs
  like `…/gmeow/Person`)
- **Vocabulary license:** [CC BY 4.0](./LICENSE-ontology) (dual-licensed — see [Licensing](#licensing))
- **Tooling license:** [Apache-2.0](./LICENSE) (dual-licensed — see [Licensing](#licensing))
- **Copyright:** © 2026 Blackcat Informatics® Inc.

> **Status.** The ontology *specification* is authored separately. This repository is the
> **tooling and infrastructure** to refine, validate, reason over, document, publish and
> version it. The `ontology/` files are a working **skeleton** (a valid header, gUFO-grounded
> module stubs) so the whole pipeline — including reasoning — runs green before the full
> specification lands.

## Quick start

```bash
make install         # sync the uv environment
make check           # full local gate: lint, validate, reason (ELK), mappings, wikidata, tests
```

`make check` requires Docker (for ROBOT). Everything else (`validate`, `mappings`,
`wikidata`, `metadata`, `apache`, `docs` via pyLODE, tests) is pure Python.

## The pipeline

| Command | What it does |
|---|---|
| `make validate` | Turtle syntax + term-annotation lint + SHACL (pure Python) |
| `make reason` | Merge import closure → OWL 2 **DL** profile check → **ELK** consistency (Docker/ROBOT) |
| `make explain` | Explain unsatisfiable classes with **HermiT** |
| `make mappings` | SSSOM → OWL/SKOS alignment axioms + VoID linksets; validates Wikidata QID syntax |
| `make wikidata` / `wikidata-live` | Wikidata QID/PID syntax gate (offline) / + existence check (network) |
| `make metadata` | Generate VoID (+ linksets) and DCAT dataset descriptions |
| `make crossref` | Generate the CrossRef DOI deposit XML (deposit schema 5.4.0) |
| `make apache` | Render the Apache content-negotiation include (`apache/gmeow.conf`) |
| `make docs` / `docs-full` | pyLODE HTML / + WIDOCO (diagrams, changelog, OOPS!) |
| `make build` | All serializations (`ttl`/`rdf`/`nt`/`jsonld`) + JSON-LD context + apache.conf → `dist/` |
| `make rdf12` | RDF 1.2 / rdf-star **preview** view via Apache Jena (gated; skips if absent) |
| `make quality` | OOPS! pitfall scan (network, best-effort) |
| `make release` | Reasoned closure (HermiT) + build + metadata + RDF 1.2 preview |

The Java tools (ROBOT, WIDOCO, Jena) run as **pinned Docker images** (see
`src/gmeow_tools/config.py`); `make pull-images` pre-pulls them. Containers run as the
invoking user, so generated files are never owned by root.

## Architecture

```
ontology/gmeow.ttl        Root ontology: metadata + owl:imports (gUFO + modules)
ontology/modules/*.ttl    Module stubs, each class grounded in a gUFO category
imports/                  Vendored gUFO (+ future extracted subsets)
catalog-v001.xml          Offline IRI→file resolution for ROBOT/Protégé
mappings/*.sssom.tsv      Cross-ontology alignments (SSSOM)
shapes/gmeow-shapes.ttl   SHACL completeness shapes
queries/                  Competency questions + QC SPARQL + RDF 1.2 projection
metadata/                 VoID + DCAT (generated)
apache/gmeow.conf         Content-negotiation include (generated)
src/gmeow_tools/          The toolchain (CLI: `gmeow …`)
```

### Reasoning: merge first

The pipeline always **merges the import closure into one ontology, then reasons/validates
that product**. ROBOT's `validate-profile` reports spurious "undeclared entity" violations
when terms are declared in a sibling imported module; collapsing to a single ontology
resolves it. ELK gates every push (fast); HermiT gates releases (sound + complete OWL 2 DL).

### Upper-ontology spine

- **gUFO** (MIT) is imported whole as the foundational categories.
- **UMBEL** (CC-BY-3.0) is intended as a *curated, extracted* reference-concept layer — never
  imported whole (it is too large for DL reasoning). Extraction is via ROBOT `extract` (SLME).
- **DOLCE/DUL** (LGPL) is **link-only** — referenced, never imported.

### Linking & the license policy

Alignments live in SSSOM (`mappings/`). Asserting a link (`owl:equivalentClass`,
`skos:exactMatch`, …) to any external term is always permitted — it copies nothing.
**Copying** axioms in (via `owl:imports` / ROBOT `extract`) is license-gated: a
reference-only source (NC/ND/share-alike/copyleft/proprietary) is **refused**
(`gmeow extract --target …`). The policy is classified by license family in
`config.py`, so new targets are classified correctly by default.

### RDF 1.2 / rdf-star

The canonical source of truth is **OWL 2 axiom annotations** (`owl:Axiom` +
`owl:annotatedSource/Property/Target`). The RDF 1.2 / rdf-star serialization is a *derived,
experimental preview* projected by Apache Jena (`make rdf12`) — the RDF 1.2 Turtle syntax is
still a W3C Working Draft, so the step is gated and the output is clearly marked non-final.

## Publishing

1. **DOI (CrossRef).** Blackcat Informatics mints the DOI as a CrossRef member (its own prefix).
   `make crossref` generates `dist/crossref-deposit.xml` (CrossRef deposit schema 5.4.0) from the
   ontology metadata, registering the DOI against the landing page `https://blackcatinformatics.ca/gmeow`.
   Set `CROSSREF_DOI_PREFIX` (and the depositor email) in `src/gmeow_tools/config.py` once
   membership is finalized — they are placeholders until then. Validate the deposit against the
   CrossRef XSD and submit on the CrossRef **test** system before depositing to production; the
   deposit is a deliberate manual step. `CITATION.cff` carries the DOI for GitHub's "Cite this
   repository" box once minted.
2. **LOD Cloud.** Submit via <https://lod-cloud.net/>. The `metadata/void.ttl` linksets supply
   the cross-dataset links the diagram needs (the ≥50-link rule).
3. **Content negotiation.** Include `apache/gmeow.conf` from the blackcatinformatics.ca vhost;
   it negotiates `Accept` → Turtle / RDF-XML / JSON-LD with an HTML fallback and per-term slash
   dereferencing. Releases are **immutable** — fix issues in a new version, never in place.

## Licensing

GMEOW is **dual-licensed**. Blackcat Informatics® Inc. is the sole copyright holder
(© 2026) and makes the work available under open-source terms **and** reserves the right
to grant separate commercial/proprietary licenses.

- **Tooling code** (this repository, excluding the vocabulary): [Apache License 2.0](./LICENSE).
- **GMEOW vocabulary** (the ontology in `ontology/` and its published serializations):
  [CC BY 4.0](./LICENSE-ontology).
- **Proprietary licensing.** The open licenses above are offered *in addition to* — not in
  place of — Blackcat Informatics' right to license either part under separate commercial
  terms. Contact `licensing@blackcatinformatics.ca`.

**Trademarks.** "Blackcat Informatics®" is a registered trademark, and "GMEOW" is a
trademark, of Blackcat Informatics Inc. Neither open license grants any right to use these
names, logos, or marks (Apache-2.0 §6; CC BY 4.0 §2(b)).

**Contributions** are accepted under the same open licenses; for the dual-licensing
reservation to extend to contributed material, contributors license their contributions to
Blackcat Informatics Inc. under terms permitting that relicensing.

**Third-party.** `imports/gufo.ttl` (gUFO) is vendored under the MIT License; its copyright
and permission notice are preserved in that file.

Full terms are in [`LICENSING.md`](./LICENSING.md); the propagating attribution and
trademark notice are in [`NOTICE`](./NOTICE).

## Conventions

`uv` for deps, `ruff` (format + lint) and `mypy --strict`, Google-style docstrings,
`pathlib.Path` everywhere, the Makefile as the canonical task runner. Missing required tools
fail loudly; the license guard and Wikidata validator error rather than silently degrade.
