<p align="center">
  <img src="./docs/gmeow-logo.svg" alt="GMEOW logo — a black cat holding a linked knowledge graph" width="160" height="160">
</p>

# GMEOW — Global Metadata and Entity Ontology for the Web

> **An LLM output is a claim, not a truth.**

<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/Blackcat-Informatics/gmeow-ontology/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology/actions/workflows/codeql.yml"><img alt="CodeQL" src="https://github.com/Blackcat-Informatics/gmeow-ontology/actions/workflows/codeql.yml/badge.svg?branch=main"></a>
  <a href="https://pypi.org/project/gmeow/"><img alt="PyPI package: gmeow" src="https://img.shields.io/pypi/v/gmeow?label=gmeow&logo=pypi&logoColor=white"></a>
  <a href="https://pypi.org/project/gmeow/"><img alt="Python versions supported by gmeow" src="https://img.shields.io/pypi/pyversions/gmeow?logo=python&logoColor=white"></a>
  <a href="./LICENSE"><img alt="Tooling license: AGPL-3.0-only" src="https://img.shields.io/badge/tooling-AGPL--3.0--only-blue"></a>
  <a href="./LICENSE-ontology"><img alt="Ontology license: CC BY 4.0" src="https://img.shields.io/badge/ontology-CC%20BY%204.0-blue"></a>
  <a href="https://doi.org/10.67342/26w4o"><img alt="DOI: 10.67342/26w4o" src="https://img.shields.io/badge/DOI-10.67342%2F26w4o-blue"></a>
</p>

GMEOW is an ontology engine for machine and human minds that treats every mental act as an
*attributed, revisable claim* rather than a stored truth — a formal place to put not just
what an agent (or a person) believes, but **who held it, from what vantage, with what
confidence, on what evidence, by which kind of reasoning, in what state of awareness, and
whether it has since been defeated.** Its flagship use is **grounded agent memory** — store
/ recall / revise — that can tell a recalled fact from a confabulated one and surfaces
disagreement as coexisting standpoints instead of overwriting the loser.

- **One claim construct, reused everywhere.** A single reified *vantage × feature × result*
  observation does the work of a measurement, a date, a categorization, an inference
  conclusion, *and* a standpoint assertion — so new capabilities add almost no new primitives.
- **Truth is never a bit.** No `isTrue`, no factive "knows." Belief is a flat lattice
  (`believes` / `doubts` / `suspends` / `accepts`) with one entailment, `knowsThat ⊑ believes`;
  truth rides a per-frame modality (□ / ◊ / refuted / "bullshit"), so contradictory claims
  coexist rather than being ranked.
- **A full model of mind.** Endurant **states** (belief, the knowing-spectrum, intention,
  emotion, metacognition) and occurrent **processes** (perceiving, reasoning, learning,
  imagining, dreaming) over **content** typed by direction-of-fit (propositions, goals,
  questions, concepts, imagined) — every faculty given a human face *and* a machine face
  (belief↔logits, memory↔context, inference↔chain-of-thought).
- **Reasoning as a first-class, inspectable act.** Deduction / induction / abduction /
  analogy, where a deduction's substrate is a real proof trace; calibration and
  reality-monitoring (over/under-confidence, known-unknowns, an `originGenerated`
  confabulation flag) are modeled directly.
- **Revision is suppression, never deletion.** A fired defeater marks the old conclusion
  `displayable false` and closes its tenure while keeping the inference as audit — which
  *is* the memory's `revise()` operation.
- **Authored once, then projected and linked widely.** Every fact is stated once across
  GMEOW's **67 self-contained slices** and generated outward as lossy projections — OWL,
  SHACL, JSON-LD, schema.org, citation and Crossref-deposit forms — while every term is
  maximally aligned to **85 external vocabularies** and authority identifiers (schema.org,
  Wikidata, PROV-O, FOAF, ORCID, DOI…), so a GMEOW graph is a first-class node in the
  linked-data and persistent-identifier web.
- **Friendly front, rigorous engine.** Flat JSON / Pydantic / MCP tools are the front door
  (no one learns RDF); reasoned RDF is the engine room.

**One engine, three products** ([v0.2.0 realignment](./docs/REALIGNMENT-v0.2.0.md), epic
[#300](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/300)):

| Product | What it is | Status |
|---|---|---|
| **`gmeow` (PyPI)** | The five-minute client and repo-free consumer CLI: inspect the bundled ontology, describe terms, verify bundles, transpile RDF, project profiles, export docs, and run the MCP server | shipped ([#296](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/296), [#442](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/442)) |
| **Grounded-memory MCP server** | `store_claim` / `recall` / `revise_belief` tool-calls for agents, backed by the claim, standpoint, evidence, and suppression model | shipped ([#297](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/297)) |
| **GTS `ai-package`** | A content-addressed, append-only, signable **single-file agent memory** — belief revision as suppression frames; portable across sessions, models, and vendors ([spec](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md)) | shipped with Python, Rust, Go, and TypeScript engines plus signing/verification ([#267](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/267), [#272](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/272), [#327](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/327)) |

**Verifiable PyPI builds.** Wheels and sdists for `gmeow` and `gmeow-gts` are built in
GitHub Actions and signed with GitHub artifact attestations. After downloading a package
from PyPI, verify it with:

```bash
gh attestation verify <path-to-wheel-or-sdist> --repo Blackcat-Informatics/gmeow-ontology
```

SPDX SBOMs are also generated for each release and attached as workflow artifacts.

**The engine** underneath is a reasoning-centric, OWL 2 DL, upper-ontology-grounded
super-vocabulary for modelling *digital existence* — people, organizations, documents,
agreements, contacts, observations, measurements, locations, rights, identity, and
contested facts — grounded in **gUFO**, **projected** down to 15+ consumer vocabularies
(schema.org, FOAF, GeoSPARQL, vCard, iCalendar, OWL-Time, ODRL, …) and **aligned by
reference** to dozens more (PROV-O, ORG, OntoLex-Lemon, Wikidata, BFO, QUDT, FALDO, IVOA,
CIDOC-CRM, …) — see the [projection](#projection-targets) and
[alignment](#aligned-by-reference) tables below. No consumer is ever required to learn RDF
to benefit ([Principle 13](./CONSTITUTION.md)); the deep model is there when you need it —
typically the first time two models disagree about a fact. The full guide set is indexed in
the [documentation map](#documentation-map).

**Why does this exist?** Because LLM and RAG outputs are stored as *truth* — no provenance,
no evidence, no confidence, no time, no way to disagree — and that is a category error with
compounding costs. See [`docs/RATIONALE.md`](./docs/RATIONALE.md) for the long form, and
the position paper *"An LLM Output Is a Claim, Not a Truth: A Substrate for Grounded Agent
Memory."*

**Principles.** Every design decision and pull request is measured against
[`CONSTITUTION.md`](./CONSTITUTION.md) — seventeen normative principles (claim-not-truth,
the-product-is-a-tool, RDF-1.2-first, one-canonical-source, maximal bridging, greenfield,
verified-by-construction, frame-relativity, suppression-never-erasure, …).
Cite them by number in issues and PRs.

- **Canonical IRI:** <https://blackcatinformatics.ca/gmeow> (slash namespace, term IRIs
  like `…/gmeow/Person`)
- **Vocabulary license:** [CC BY 4.0](./LICENSE-ontology) (dual-licensed — see [Licensing](#licensing))
- **Tooling license:** [AGPL-3.0-only](./LICENSE) (dual-licensed — see [Licensing](#licensing))
- **Copyright:** © 2026 Blackcat Informatics® Inc.

**Four things GMEOW does that no agent-memory store does:**

- **Statement-level provenance & confidence.** RDF 1.2 / RDF\*-first: every fact is an
  attributed, confidence-weighted, time-scoped claim, downcast losslessly to OWL axiom
  annotations for reasoners ([Principles 2–3](./CONSTITUTION.md); see *RDF 1.2* below).
  A sensor reading, a human assertion, and a model output are the *same reified construct*.
- **Contested facts without a winner.** Disputed facts — including two models disagreeing —
  are recorded as coexisting, standpoint-indexed claims, never collapsed to a preferred,
  ranked, or latest value ([`docs/standpoints.md`](./docs/standpoints.md)).
- **Forgetting with an audit trail.** Belief revision is supersession + suppression
  (`gmeow:displayable false`), never deletion: the superseded belief is withheld from every
  projection and recall path, and retained — with when, on whose say-so, and why — forever
  ([Principle 10](./CONSTITUTION.md)).
- **Identity, naming & display safety — for human *and* digital subjects.** Names and
  identity are reified, co-equal and self-asserted — no `primaryName`/`preferredGender`,
  deadnames suppressed-not-deleted, a 7-axis orthogonality matrix (pronouns ⟂ honorifics ⟂
  gender identity ⟂ expression ⟂ sex ⟂ sexual ⟂ romantic orientation) **enforced by
  tests** — and self-assertion outranks any inference, for people and for AI entities alike
  ([Principles 9 & 16](./CONSTITUTION.md); [`slices/core/names/docs.md`](./slices/core/names/docs.md),
  [`docs/identity-mapping.md`](./docs/identity-mapping.md)). Identity and deception
  epistemics ship in the **core**, by commitment — a memory substrate that makes "what is a
  person" an optional add-on has already answered the question, badly.

> **Status.** GMEOW now ships as **62 slices**, each with a full guide; 61 slice
> examples exercise the model and feed the full GTS bundle. The current surface covers
> identity (entities, names, gender, sexuality, languages), social/contact/email/account
> data, content/evidence/software, trust/attestation/crypto, skills, cognition,
> epistemics, agreements, rights, norms, risk, place/time/events, music, narrative,
> research objects, and the frame-relative observation/measurement spine. The logic layer
> is native RDF 1.2 first: OWL, Datalog, N3, Prolog, probabilistic, counterfactual, and
> profile-certified forms are projections of the same `logic:` source. The full
> toolchain (validate → reason → regenerate → transpile acceptance → docs → publish)
> is registry-gated and source-derived, with no hand-edited generated artifacts.

## Documentation map

Every guide under [`docs/`](./docs/) (plus the two root governance documents). **Doctrine**
docs explain a cross-cutting design commitment; **domain guides** (`*-mapping.md`) teach one
slice's model *and* how it aligns/projects.

| Guide | Kind | What it covers |
|---|---|---|
| [`CONSTITUTION.md`](./CONSTITUTION.md) | Governance | The seventeen normative principles every design decision and PR is measured against |
| [`docs/REALIGNMENT-v0.2.0.md`](./docs/REALIGNMENT-v0.2.0.md) | Governance | The v0.2.0 realignment: one engine, three products — positioning, recast inventory, deliverables D1–D7 |
| [`docs/RATIONALE.md`](./docs/RATIONALE.md) | Doctrine | Why GMEOW exists — the nine challenges of digital existence and the architectural answers |
| [`docs/mcp-server.md`](./docs/mcp-server.md) | Product | The MCP server: the grounded-memory triad (`store_claim`/`recall`/`revise_belief`) + the ontology toolchain tools; one-line install |
| [`docs/hallucination-resistant-kg.md`](./docs/hallucination-resistant-kg.md) | Doctrine | The claim-extraction spine done right — fixture, prompt, audit gates, `gmeow audit`; scored across models on the [eval leaderboard](./generated/evals/leaderboard.md) |
| [`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md) | Specification | The Graph Transport Substrate — the content-addressed, append-only single-file format behind the `ai-package` memory and the narrow-waist exports |
| [`docs/VERIFY-EXAMPLE.md`](./docs/VERIFY-EXAMPLE.md) | Reference | Sample signed `gmeow.gts` verification output: signature counts, transport-key fingerprint, emoji hash, randomart, and bundled ontology checks |
| [`docs/cli-extensions.md`](./docs/cli-extensions.md) | Specification | The `gmeow` CLI extension roll-up — subcommand discovery from slice manifests, GTS profile gating, and solver-layer transforms |
| [`slices/core/logic/design/LOGIC.md`](./slices/core/logic/design/LOGIC.md) | Doctrine | The native RDF 1.2 `logic:` layer: canonical logic source, projection profiles, conformance, runtime, and migration |
| [`docs/reasoning.md`](./docs/reasoning.md) | Doctrine | The OWL-infers / SHACL-validates split, the four verification lanes, and why OWL cardinality is avoided |
| [`docs/projections.md`](./docs/projections.md) | Doctrine | The four-artifact alignment stack (SSSOM / EDOAL / FnO / SPARQL) and how lossy down-projection works |
| [`docs/transpile.md`](./docs/transpile.md) | Doctrine | The full transpile — consumer RDF → pure-GMEOW draft → MAXIMAL multi-vocab; `gmeow transpile`, stdin streaming, and the draft as a first-class artifact |
| [`docs/foundational-bridging.md`](./docs/foundational-bridging.md) | Doctrine | The gUFO ↔ BFO 2020 foundational-spine bridge, by reference (Principle 5 applied recursively) |
| [`docs/import-provenance.md`](./docs/import-provenance.md) | Doctrine | How external vocabularies are sourced; the IMPORT_OK vs reference-only license policy and carrier-time |
| [`docs/CITATIONS.md`](./docs/CITATIONS.md) | Doctrine | The canonical citation ledger, generated bibliography exports, and agent maintenance rule |
| [`docs/standpoints.md`](./docs/standpoints.md) | Doctrine | Contested facts as coexisting, standpoint-indexed claims — no privileged winner |
| [`docs/rights.md`](./docs/rights.md) | Doctrine | Rights / IP / licensing as reified, temporally-bound, machine-readable claims (ODRL superset) |
| [`docs/temporal-queries.md`](./docs/temporal-queries.md) | Reference | TQL — the parameterized temporal query algebra (Allen relations) over the events/temporal model |
| [`slices/core/names/docs.md`](./slices/core/names/docs.md) | Domain guide | Names as reified, co-equal, anti-colonial relationships; pronouns & honorifics as first-class facets |
| [`docs/identity-mapping.md`](./docs/identity-mapping.md) | Domain guide | Gender & sexuality as orthogonal, self-asserted, co-equal facets (the 7-axis matrix) |
| [`slices/extensions/languages/docs.md`](./slices/extensions/languages/docs.md) | Domain guide | Languages as registry-independent first-class entities; co-mingled writing systems; proficiency |
| [`slices/extensions/email/docs.md`](./slices/extensions/email/docs.md) | Domain guide | Email message/header structure, participants, and RFC 5322 mapping; time-scoped address tenure |
| [`docs/location-mapping.md`](./docs/location-mapping.md) | Domain guide | The universal reference-frame: 13+ realms, RCC-8 topology, pose/trajectory, frame-relativity |
| [`docs/music-mapping.md`](./docs/music-mapping.md) | Domain guide | Music as frame-relative content: WEMI, tuning/time frames, notation as declared-loss projection, performance, timbre, genre, and analysis standpoints |
| [`slices/core/attestation/docs.md`](./slices/core/attestation/docs.md) | Domain guide | Signed-claim envelopes, verification results, and append-only transparency logs (cross-cutting) |
| [`slices/core/rights/docs.md`](./slices/core/rights/docs.md) | Domain guide | Alignment/projection companion to `rights.md` — ODRL, CC REL, Dublin Core, SPDX, schema.org |
| [`slices/core/standpoint/docs.md`](./slices/core/standpoint/docs.md) | Domain guide | Alignment/projection companion to `standpoints.md` — CRMinf, PROV-O, Web Annotation, schema:Claim |
| [`slices/core/versions/docs.md`](./slices/core/versions/docs.md) | Domain guide | Versions as standpoint-scoped claims (latest / stable / yanked / canonical are not intrinsic) |
| [`docs/wikidata-mapping.md`](./docs/wikidata-mapping.md) | Domain guide | The Wikidata integration policy — `wd:` / `wdt:` / `ps:` / `pq:` semantics; QID/PID syntax gates |
| [`docs/BRAND.md`](./docs/BRAND.md) | Brand | Logo usage and trademark guidelines |

## Quick start

**Using GMEOW** (no source checkout required):

```bash
pip install gmeow gmeow-gts

gmeow info
gmeow describe gmeow:StandpointClaim
gmeow transpile source.ttl --profiles all -o out/
gmeow docs --directory docs-tree
gmeow mcp
```

The public `gmeow` CLI is backed by the bundled `generated/dist/gmeow.gts` snapshot,
so description, verification, docs, transpile, projection, export, CrossRef metadata,
and GTS conversion run from the wheel. Repository maintenance stays on `gmeow-dev`:
if a command needs `dsl/`, `slices/`, `generated/`, Docker, or dev fixtures, it is a
developer command.

**Working on the engine** (the ontology, compilers, and gates):

```bash
make install         # sync the uv environment
make check           # fast local gate: lint, validate, ELK, mappings, wikidata, fast tests
make check-docker    # optional Docker gate: HermiT, reasoning cases, Jena statement checks
```

`make check` is the normal local gate. Docker-heavy lanes are explicit so routine
development is not blocked on HermiT/Jena, while CI and release jobs still exercise the
complete reasoner surface.

## The `gmeow.gts` bundle

GTS exists because grounded memory cannot be just an RDF dump, a database export, or a
tarball. A useful agent-memory package has to move as one file, preserve RDF 1.2
statement metadata, carry the binary evidence and docs the graph references, survive
append-only revision, compose without reserialization, and be mechanically verifiable
offline. GTS is that narrow waist: the ontology, claim layer, evidence blobs, projection
surface, and verification trail travel together, while readers remain small enough to
implement independently in Python, Rust, Go, and TypeScript.

GMEOW is the primary package. GTS is the transport utility we also ship, and the same
conformance corpus gates its four reader implementations:

| Runtime | Distribution | Install |
|---|---|---|
| Python | [PyPI `gmeow-gts`](https://pypi.org/project/gmeow-gts/) | `pip install gmeow-gts` |
| Rust | [crates.io `gmeow-gts`](https://crates.io/crates/gmeow-gts) | `cargo install gmeow-gts` |
| TypeScript | [npm `@blackcatinformatics/gmeow-gts`](https://www.npmjs.com/package/@blackcatinformatics/gmeow-gts) | `npm install @blackcatinformatics/gmeow-gts` |
| Go | [pkg.go.dev `go.blackcatinformatics.ca/gts`](https://pkg.go.dev/go.blackcatinformatics.ca/gts) | `go install go.blackcatinformatics.ca/gts/cmd/gts@latest` |

The committed [`generated/dist/gmeow.gts`](./generated/dist/gmeow.gts) artifact is the
repo-free GMEOW distribution snapshot. It is the file bundled into the wheel and used by
`gmeow info`, `describe`, `docs`, `verify`, `transpile`, `project`, and the GTS export
paths when no source checkout is present. The current dist snapshot folds as one
`dist`-profile segment with 18,207 terms, 33,142 quads, 116 RDF 1.2 reifiers, 324
statement annotations, and 68 content-addressed blobs.

What rides inside that single file:

- The import-free authored GMEOW graph as the default graph, plus the vendored gUFO/import
  closure and self-description metadata as named graphs.
- The RDF 1.2 statement layer, so provenance, confidence, time scope, standpoints, and
  suppression metadata survive transport rather than collapsing into flat triples.
- The alignment and transform surface: SSSOM mappings, projection CONSTRUCT queries,
  mapping cells, and the denied-cell ledger needed by repo-free transpile/projection.
- The docs surface: every slice guide, the project `docs/` tree, and the generated
  ontology-docs site as content-addressed blobs that `gmeow docs` can extract from the
  installed package.

The Graph Transport Substrate underneath is deliberately small and mechanical: a CBOR
append-only segment log; deterministic BLAKE3 frame IDs and `prev` chains; a four-table
RDF 1.2 fold (`terms`, `quads`, `reifies`, `annot`); content-addressed binary blobs;
suppression frames for belief revision; literal byte concatenation (`cat 2.gts >> 1.gts`),
with `gts cat` as the validating composer; files-profile pack/unpack/diff; N-Quads,
SQLite, and DuckDB transforms; and robust reader diagnostics for torn appends, damaged
frames, broken chains, unknown codecs, missing keys, conflicting reifiers, and position
constraints.

Verification has two layers. `gts verify generated/dist/gmeow.gts` verifies the GTS chain
and reports the composition ledger. `gmeow verify <bundle.gts>` adds source-free ontology
checks over the folded graph: namespace, term catalog, missing labels/definitions, reader
diagnostics, and documentation blobs. Signed release bundles also carry COSE signatures and
an embedded OpenPGP transport key; `gmeow verify` displays the grouped fingerprint, emoji
hash, text labels, randomart, and valid/invalid/unverified signature counts. See
[`docs/VERIFY-EXAMPLE.md`](./docs/VERIFY-EXAMPLE.md) for the expected signed-output shape.

## The pipeline

| Command | What it does |
|---|---|
| `make validate` | Turtle syntax + term-annotation lint + SHACL (pure Python) |
| `make reason` | Merge import closure → OWL 2 **DL** profile check → **ELK** consistency (Docker/ROBOT) |
| `make explain` | Explain unsatisfiable classes with **HermiT** |
| `make verify` | Reasoned-graph SPARQL QC (ROBOT `verify` over `queries/verify/`) — the closed-world half of the [OWL-infers / SHACL-validates split](./docs/reasoning.md) |
| `make regenerate` | Rebuild EVERY committed artifact under `generated/` via the registered-generator framework (#279): mappings, projections, statements, schemas, lpg, metadata, apache, the module-status matrix |
| `make check-generated` | Drift + orphan + internal-tag-leak gate over every registered generator |
| `make mappings` | SSSOM → OWL/SKOS alignment axioms + VoID linksets; validates Wikidata QID syntax |
| `make wikidata` / `wikidata-live` | Wikidata QID/PID syntax gate (offline) / + existence check (network) |
| `make crossref` | Generate the CrossRef DOI deposit XML (deposit schema 5.4.0) |
| `make acceptance` | Score full transpile on real external RDF snapshots; hard gates plus honest coverage scoreboard |
| `make docs` / `docs-full` | Native ontology-docs site into `dist/ontology-docs` / + optional Docker stages |
| `make build` | All serializations (`ttl`/`rdf`/`nt`/`jsonld`) + JSON-LD context → `dist/` (ephemeral) |
| `make check-docker` | HermiT, reasoning cases, and Jena-backed statement checks |
| `make quality` | OOPS! pitfall scan (network, best-effort) |
| `make release` | Regenerate + HermiT closure + build + compliance report + CrossRef deposit |

The Java tools (ROBOT, WIDOCO, Jena) run as **pinned Docker images** (see
`src/gmeow_tools/config.py`); `make pull-images` pre-pulls them. Containers run as the
invoking user, so generated files are never owned by root.

## Architecture

**The one rule (#287):** under `generated/`, a registered generator owns it; under
`dist/`, it is ephemeral; everything else is authored. The unit of the ontology is
the **slice** — identical anatomy for core and extensions, with the manifest as the
sole source of identity and tier (Principles 15–16).

```text
slices/<group>/<name>/    A slice: manifest.ttl (IRI + tier + deps + consumer),
                          module.ttl, shapes.ttl, mappings/, queries/, tests/,
                          docs.md. The <group> dir (core/, extensions/) is human
                          organization only — the build reads manifests.
slices/vocabulary.ttl     The slice-manifest authoring vocabulary (spec layer)
ontology/gmeow.ttl        Root ontology: metadata + owl:imports (gUFO + slices)
dsl/mappings/             Mapping DSL: vocabulary, foundational gUFO↔BFO bridge,
                          per-target projections, transforms.fno.ttl
dsl/statements/           The canonical RDF 1.2 / RDF* statement-metadata source
shapes/                   Authored SHACL (incl. the slice-manifest shapes)
queries/                  Authored SPARQL: competency/, verify/, qc/, codecs/
imports/                  Vendored gUFO + validation-only axiom snapshots
catalog-v001.xml          Offline IRI→file resolution for ROBOT/Protégé
src/gmeow_tools/          The toolchain (CLI: `gmeow …`)

generated/                EVERY committed generated artifact — one root, every
                          path owned by a registered generator (drift-, orphan-,
                          and internal-tag-leak-gated):
                          mappings/ (SSSOM) · projections/ (EDOAL+FnO) ·
                          queries/ (projection CONSTRUCTs) · statements/
                          (RDF 1.2 lead + OWL downcast) · schemas/ · lpg/ ·
                          metadata/ (VoID+DCAT) · apache/ · module-status.md
dist/                     Ephemeral build products (never committed)
```

The per-slice audit state — tier, dependencies, term counts, documentation
status — is the generated [`generated/module-status.md`](./generated/module-status.md).

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
- **Foundational bridging (the spine reaches outward).** gUFO's *nature* categories are aligned
  by reference to **BFO 2020** (ISO/IEC 21838-2) — `skos:closeMatch`, never imported — so GMEOW
  interoperates with the OBO-Foundry / ISO top-level world. This is Principle 5 applied
  recursively to the foundational layer; the emitted BFO IRIs are verified against a vendored
  class snapshot (`imports/targets/bfo.ttl`), kept out of the reasoned closure. DOLCE/SUMO are
  link-only bridge views, not imported truth sources. Authoring source: `dsl/mappings/foundational/`; full guide:
  [`docs/foundational-bridging.md`](./docs/foundational-bridging.md).

### Linking & the license policy

Alignments are authored once in the **mapping DSL** (`dsl/mappings/`) and compiled
(`gmeow-dev regenerate mappings`) to SSSOM + EDOAL + FnO + SPARQL — see [§ The mapping
compiler](#the-mapping-compiler). Asserting a link (`owl:equivalentClass`,
`skos:exactMatch`, …) to any external term is always permitted — it copies nothing.
**Copying** axioms in (via `owl:imports` / ROBOT `extract`) is license-gated: a
reference-only source (NC/ND/share-alike/copyleft/proprietary) is **refused**
(`gmeow-dev extract --target …`). The policy is classified by license family in
`config.py`, so new targets are classified correctly by default.

### The mapping compiler

GMEOW's doctrine — *one canonical source, everything else a generated lossy
projection* ([Principle 4](./CONSTITUTION.md)) — applies to the alignment layer
itself. Every mapping is authored
**once** as a `gmeow:`-grounded Turtle cell in `dsl/mappings/`, and
`gmeow-dev regenerate mappings` renders the four standard artifacts (SSSOM term links, EDOAL
complex cells, FnO transform functions, SPARQL CONSTRUCT executors). Drift is
impossible by construction; `gmeow-dev check-generated mappings` is the CI no-drift
gate. The compiler uses each target language to its full extent (EDOAL
`compose`/`inverse` relation paths, FnO `fnom` implementation linkage, SSSOM
provenance + labels, the full SPARQL path/expression algebra) — all expressed as
GMEOW vocabulary, never raw SPARQL. Full reference + authoring guide:
[`docs/projections.md`](./docs/projections.md).

### Projection targets

GMEOW **projects down** to the vocabularies people actually consume — a deliberately lossy,
directional export that downgrades the rich canonical model into a target consumer's terms
without corrupting it ([Principle 4](./CONSTITUTION.md)). Each target below is authored in
`dsl/mappings/projections/`, compiled to an EDOAL spec (`generated/projections/*.edoal.ttl`) +
a SPARQL CONSTRUCT executor (`generated/queries/*.rq`), and run by `gmeow project` /
`make project`. The full set with worked examples lives in
[`docs/projections.md`](./docs/projections.md).

| Target | Spec | GMEOW exports… |
|---|---|---|
| **schema.org** | <https://schema.org> | The flat contact-card surface: `Person`/`Organization`/`Place`, reconstructed `name`/`birthDate`/`jobTitle`/`gender` from reified structures, plus `accessibilityFeature`/`accessibilityHazard` from the accessibility facet layer |
| **FOAF** | <http://xmlns.com/foaf/0.1/> | The lowest-common-denominator person/agent graph: `name`, `nick`, `homepage`, `mbox`, `knows` |
| **vCard (RDF)** | <https://www.w3.org/TR/vcard-rdf/> | Contact cards: `fn`, `given-name`/`family-name`, `nickname`, `bday`, `title`, `hasAddress`, `hasURL`, `hasGeo`, and free-text `vcardx:pronouns` (RFC 9554) |
| **GeoSPARQL** | <https://www.ogc.org/standard/geosparql/> | `geo:asWKT` geometry literals and topology from `Place`/`Location` + frame-relative coordinates |
| **iCalendar (RDF)** | <https://www.w3.org/TR/rdfcal/> | Calendar projections of events — `Vevent`, `dtstart`/`dtend`, summary |
| **OWL-Time** | <https://www.w3.org/TR/owl-time/> | `time:Instant`/`Interval` and Allen relations from the temporal model |
| **ODRL** | <https://www.w3.org/TR/odrl-model/> | Pure ODRL policies — `Permission`/`Prohibition`/`Duty`, the action vocabulary + constraint algebra |
| **Creative Commons REL** | <https://creativecommons.org/ns> | `cc:license`/`cc:permits`/`cc:prohibits`/`cc:requires` from rights statements |
| **SPDX** | <https://spdx.org/rdf/terms/> | SPDX license identifiers and licensing facts for software/data artifacts |
| **Dublin Core Terms** | <https://www.dublincore.org/specifications/dublin-core/dcmi-terms/> | `dcterms:` metadata — title, creator, date, rights, license |
| **BOT** | <https://w3id.org/bot> | Building-topology projection of indoor places — `bot:Zone`/`Element`/`hasSpace` |
| **RDF Data Cube** | <https://www.w3.org/TR/vocab-data-cube/> | Well-formed `qb:Observation` + `qb:DataSet` + `qb:DataStructureDefinition` — a statistical-cube projection of spatial aggregations (IC-1, IC-2) |
| **OntoLex-Lemon** | <https://www.w3.org/2016/05/ontolex/> | `ontolex:LexicalEntry`/`Form`/`writtenRep` from appellations and language data |
| **W3C Web Annotation** | <https://www.w3.org/TR/annotation-vocab/> | `oa:Annotation` body/target projection (tags, standpoints) |
| **Standpoint projections** | [`docs/standpoints.md`](./docs/standpoints.md) | Five frame-preserving exports of contested claims: **CRMinf**, **PROV-O**, **schema:Claim**, **Web Annotation**, **Standpoint-OWL 2** — never one that picks a winner |

### Aligned by reference

Beyond what it projects, GMEOW **aligns by reference** (`skos:exactMatch` / `closeMatch` /
`owl:equivalentClass`, copying no axioms — [Principle 5](./CONSTITUTION.md)) to dozens more
vocabularies, so data already published elsewhere is covered without rewriting. The
**exhaustive, authoritative list is the SSSOM output set** under
[`generated/mappings/`](./generated/mappings/) (one table per slice/domain); this is a
representative, grouped sample:

| Domain | Aligned vocabularies (by reference) |
|---|---|
| **Foundational** | gUFO, **BFO 2020** (ISO/IEC 21838-2), DOLCE/SUMO bridge views |
| **Hub & coreference** | **Wikidata**, schema.org, FOAF, ORG, PROV-O |
| **Identity & language** | GSSO, Homosaurus, FHIR, FOAF, OntoLex-Lemon, LIME, Glottolog, CEFR/ILR/ACTFL |
| **Geospatial & place** | GeoSPARQL, CIDOC-CRM + CRMgeo, BOT/ifcOWL, LADM, INSPIRE, AIXM, UNCLOS, MRGID, OGC GeoPose, OGC Moving Features |
| **Scientific & measurement** | **QUDT**, SOSA/SSN + SensorThings, **IVOA**/UAT/SWEET (astronomy), **FALDO**/Sequence Ontology/GFF3 (genomics), IEEE 1872-2015 (robotics), OpenMath/MEX (mathematics), W3C DQV + ISO 19157 (data quality) |
| **Rights & provenance** | ODRL, CC REL, Dublin Core, SPDX, RightsStatements.org (all 12), PREMIS 3, WIPO, W3C Media Resources |
| **Trust & attestation** | PROV-O, in-toto, SLSA, DSSE, Sigstore/Rekor, SCITT, nanopublications |
| **Privacy & content** | W3C DPV, SKOS, MOAT, Web Annotation, RDF Data Cube |
| **Finance** | FIBO (by reference), schema.org, Dublin Core, provenance and rights surfaces |

### RDF 1.2 / RDF\* — the canonical statement-level model

GMEOW is **RDF 1.2 / RDF\*-first** ([Principles 2–3](./CONSTITUTION.md)): statement-level
metadata — provenance, confidence, temporal scope — is **authored once** as native RDF 1.2 /
RDF\* content in `dsl/statements/`, the canonical source. From it `gmeow-dev regenerate statements`
generates two verified artifacts: the **RDF 1.2 / RDF\* serialization** (the lead form, via
Apache Jena — the only engine that emits triple terms today) and the **OWL 2 axiom-annotation
form** (`owl:Axiom` + `owl:annotatedSource/Property/Target`) — the *generated,
reasoning-lossless downcast* that the OWL 2 DL reasoners GMEOW gates on actually consume. The
OWL form is the **downgrade for legacy tooling** — the same lossy-compatibility-as-projection
principle GMEOW applies to schema.org / vCard / FOAF ([Principle 4](./CONSTITUTION.md)), not a
competing source of truth — and it recedes as RDF-1.2-native reasoners and stores arrive. Both
downcasts are guarded by `make check-generated`
([Principle 7](./CONSTITUTION.md)). The scope is exact: the **logical TBox stays OWL 2 DL** —
triple-terms are not OWL 2 DL, and GMEOW never claims otherwise.

### Native logic: OWL is a projection, not the ceiling

GMEOW's logical core is a canonical RDF 1.2 `logic:` layer
([Principle 17](./CONSTITUTION.md)). The authored logic source
normalizes into a typed IR, then projects into the forms each engine can consume:
OWL DL/EL for today’s reasoners, Datalog/Nemo for monotonic materialization, N3 and Prolog
for rule/backward-goal surfaces, plus preservation/loss ledgers that make every downgrade
auditable. The Rust `crates/logic` engine stores world-indexed graphs in oxigraph, uses
Nemo for forward materialization, carries proof traces, and exposes PyO3/wasm seams. Logic
profiles certify what is decidable, complete, lossy, probabilistic, counterfactual, or
budget-bounded before anything is allowed to call itself preserved. Design entrypoint:
[`slices/core/logic/design/LOGIC.md`](./slices/core/logic/design/LOGIC.md).

### Names: first-class, multi-culture, inclusive

Most vocabularies treat a name as a flat string (`familyName`). GMEOW models it as a
**reified, time-bounded, context-scoped, source-attributed relationship** — a
`gmeow:Appellation` borne by an entity, with the `gmeow:NameUsage` relator binding *who is
named × which name × toward whom × in what register × over what period*. That makes naming
non-standard in deliberate, useful ways (full rationale in
[`slices/core/names/docs.md`](./slices/core/names/docs.md)):

- **Co-equal, anti-colonial.** A person's names in different languages/scripts (e.g.
  *Patrick Colm Audley* and *欧德理*) are **co-equal full names** — neither is the other's
  alternate or romanization, and **there is no `primaryName`/`preferredForDisplay` term**.
  Display selection is locale-relative and symmetric; self-asserted names are top authority.
- **Genuinely multi-cultural.** Name parts are an open value vocabulary — patronymic, Arabic
  *ism/kunya/nasab/laqab/nisba*, Spanish double surname, East-Asian generation & clan names,
  Balinese birth-order, Roman *nomina*, mononyms — with **no forced given+family order**.
- **Contextual & temporal.** "Aunt Genny" (family) vs "Mrs Smith" (students) coexist via
  `NameUsage`; name changes, and deadnames are recorded yet suppressed from display.
- **Pronouns & honorifics** are first-class, contextual, and **independent of sex/gender**.
  Pronoun sets are a **maximal, source-cited anchor inventory** (21 stably-declinable English
  sets — she/her … fae/faer, ze/zir, thon, xe/xem, …; declensions verified against
  [pronouns.page](https://en.pronouns.page)) plus an explicit **name-only / no-pronouns** value,
  with open minting for anything unseeded. They link to Wikidata's *personal pronoun set*
  (`wd:Q65067284`/`wdt:P6553`) and **project** to the vCard 4 PRONOUNS property (RFC 9554). Appellations subclass OntoLex-Lemon `ontolex:LexicalEntry` (`gmeow:fullName` close-matches `ontolex:writtenRep`), projecting to OntoLex Form/writtenRep structures.

### Languages: registry-independent, conlang- & AI-ready

Most vocabularies treat a language as an opaque tag (`inLanguage "ja"`) — *a language **is**
its ISO/BCP-47 code*. GMEOW inverts that: a **`gmeow:Language` has a self-minted IRI**,
registry codes are optional alignments (never identity), and **internal string literals use private-use BCP-47 tags (e.g., `@x-gmeow-japanese`)** to isolate GMEOW graphs from external registries. Standard BCP-47 tags are reconstructed on-demand during down-projection. Full rationale is in
[`slices/extensions/languages/docs.md`](./slices/extensions/languages/docs.md):

- **Registry-independent.** A code-less conlang (**Ithkuil**), a fast-versioning AI-minted
  interlingua, an under-coded sign/minority language, and a programming language are all
  **co-equal first-class languages**. BCP-47/ISO/Glottolog/Wikidata attach *when they exist*,
  as `gmeow:authorityLink`/`skos:exactMatch` — and standard BCP-47 tags are **reconstructed on demand**
  by the projection layer (`ja`+`Hani` → `ja-Hani`). Properties like `gmeow:nameLanguage` close-match LIME's metadata property `lime:language` to map first-class language objects to standard tags on demand.
- **Co-mingled writing systems.** A language uses many co-equal scripts at once: Japanese
  interleaves kanji, hiragana, katakana and rōmaji, each in a distinct *role*, via the reified
  `gmeow:WritingSystemUsage` relator (which also models script changes over time). Bespoke and
  non-linear conlang scripts are first-class.
- **First-class version lineage** (Ithkuil 1993/2011/New; AI v1→v2), **AI/software creators**,
  and **reified per-skill proficiency** (CEFR/ILR/ACTFL — "native overall" and "B2 writing"
  coexist).
- **Transformations are functions.** Transliteration/transcription/translation (Hepburn,
  Pinyin, IPA, …) are declarative **FnO functions**, so a romanization records *how* it was
  derived. Flat "First Last" / `schema:knowsLanguage` renderings are **downcast projections**,
  never canonical clutter.

### Gender & sexuality: orthogonal, self-asserted, inclusive

Most data models cram a person into one `gender` string — conflating things that are
*independent* and erasing self-determination and change. GMEOW models gender and
sexuality as **reified, self-asserted, co-equal facets** on a shared
`gmeow:IdentityFacet` (a `gufo:Relator`, the `NameUsage` idiom), across two modules
(full rationale in [`docs/identity-mapping.md`](./docs/identity-mapping.md)):

- **Orthogonal axes, proven.** Address (pronouns/honorifics — in the names module),
  **gender identity**, **gender expression**, **sex assigned at birth**, and — split
  apart — **sexual** and **romantic** orientation are independent. A 7-axis
  **orthogonality matrix is enforced by tests**: no axis is inferred from another.
  *What you want to be called ≠ what you are; sex ≠ gender; asexual yet biromantic is
  expressible.*
- **Self-assertion is the top authority**, and identities are **co-equal** — bigender
  is two facets, neither primary. There is no `primaryGender`; a superseded label
  (a former gender, like a deadname) is kept with `gmeow:displayable false` —
  recorded yet **never displayed, never deleted**.
- **Inclusive without overtyping.** Gender and orientation are **open value
  vocabularies of individuals** (woman, non-binary, agender, Two-Spirit, …;
  bi/pan/ace/aro/…) — never per-value `Person` subclasses, never a forced enum. An
  identity not yet seeded is a **fresh value individual with a label**, the single
  path — no flat-literal shortcut.
- **Honestly interoperable.** Values align (lossily) to GSSO, Homosaurus, Wikidata
  (`P21`/`P91`), schema.org, FOAF and FHIR — every identifier verified against the
  source. Displayable gender projects to `schema:gender`/`foaf:gender`; suppressed
  labels never leak, and orientation is a documented lossy drop.

### Standpoints: contested facts that coexist, no winner

A flat model gives a disputed fact **one slot** two parties must both own — so they
edit-war over it. GMEOW records a contested fact as **several standpoint-indexed
claims that coexist, none privileged** (full rationale in
[`docs/standpoints.md`](./docs/standpoints.md)):

- **Three orthogonal axes.** `gmeow:accordingTo` (*whose frame* — the standpoint) is
  held apart from `gmeow:wasAttributedTo` (*which source* recorded it) and
  `gmeow:confidence` (*how sure* we are); a neutral source can record a partisan
  claim. The axis is an annotation property, so the OWL downcast stays OWL 2 DL.
- **Two clocks.** Fact-time (`validFrom`/`validUntil`, when the fact holds) is kept
  distinct from standpoint-time (a `gmeow:StandpointTenure`, when the frame held the
  position — recognition granted then withdrawn, suppressed not deleted).
- **No single slot to win.** There is no `preferredRank`/`primary*` — refused three
  ways (a SHACL shape, a statement-DSL lint, and a term-absence test). Crimea
  contained-in Russia *and* Ukraine coexist, neither privileged, and the reasoned
  graph stays consistent.
- **At least as expressive as CRMinf, formally grounded, projected losslessly.** The
  facility realises **Standpoint Logic** (`gmeow:standpointModality` spans □/◊ *and*
  the CRMinf belief value true/probable/possible/**false**, so a standpoint's *denial*
  is first-class; `gmeow:sharpens` = the standpoint poset; `gmeow:universalStandpoint`
  = the universal `*`). Five projections — **Standpoint-OWL 2** (`standpointLabel`,
  for a standpoint reasoner), **CRMinf** (the CIDOC-CRM argumentation/belief model),
  **PROV-O** (qualified attribution), **W3C Web Annotation**, and **schema.org Claim**
  — each preserve every frame. There is deliberately **no** projection that selects
  one standpoint: collapsing a contested fact to a chosen frame is picking a winner.

### Rights & IP: instance-level, machine-readable, temporally bound

Most vocabularies record rights as a flat `license` URL or a `rights` string. GMEOW
models the rights of *any* instance — a work, image, brand, dataset, software project —
as a **reified, attributed, temporally-bound, machine-readable** facility, distinct from
the build-time `LinkPolicy` that governs copying axioms *into* GMEOW (full rationale in
[`docs/rights.md`](./docs/rights.md)):

- **A licence *is* an agreement, a holder *is* an agent.** `gmeow:License ⊑
  gmeow:Agreement` reuses `gmeow:hasParty`; `gmeow:copyrightHolder` / `trademarkHolder`
  specialise `gmeow:wasAttributedTo` — no parallel models. A reified `gmeow:Copyright`,
  `gmeow:Trademark` (mark × holder × registration × ™/®/status) and `gmeow:RightsStatement`
  carry the structure; flat `gmeow:hasLicense` / `hasCopyright` covers the 80 % case.
- **The deontic logic, not just the structure.** `gmeow:RightsStatement` is an
  ODRL-superset policy: `gmeow:Permission` / `gmeow:Prohibition` / `gmeow:Duty` over the
  **full ODRL action vocabulary**, the **constraint algebra** (atomic *dateTime ≤ 2036* /
  *spatial = EU* + logical and/or/xone), conflict-resolution strategy and consequence/
  remedy chaining. Licences are **temporally bound** (`validFrom`/`validUntil` + dateTime
  constraints); claims carry **provenance/confidence/standpoint** (the RDF-1.2 layer);
  expired rights are suppressed, never deleted.
- **Maximal superset, by reference.** One canonical term per concept, aligned to **ODRL,
  CC REL, Dublin Core, schema.org, SPDX, RightsStatements.org (all 12), PREMIS 3, W3C
  Media Resources, WIPO/Wikidata** (every QID curl-validated) — and **projected** to pure
  ODRL, CC REL, schema.org, Dublin Core and SPDX. IPROnto and MPEG-21 REL are bridged by
  reference (no fabricated IRIs). Foundational: the Images and Employment blocks build on it.

### Locations: universal reference-frame

Most vocabularies model location as a flat geographic point (`latitude`, `longitude`).
GMEOW treats **Location as a relationship between an entity and a reference frame** — one
kernel locates a coffee cup, a satellite, a neural embedding, a gene on a genome, and a
wizard's tower (full rationale in [`docs/location-mapping.md`](./docs/location-mapping.md)):

- **Universal reference-frame kernel.** `gmeow:Location` is the umbrella; structural kinds
  (`Place`, `VirtualLocation`, `StorageLocation`, `CelestialLocation`, `BiologicalSequenceLocation`)
  are subclasses where structure differs. Kinds within each kind are open value vocabularies
  (`placeType`, `celestialObjectType`, `sequenceFeatureType`), not subclasses — any granularity
  from country to room, from star to galaxy cluster, or from chromosome to SNP, can be a
  first-class entity.
- **Frame-relativity by construction (Principle 11).** Every coordinate, measurement, or pose
  is expressed in an explicit `gmeow:ReferenceFrame` — a self-describing Profile with closed
  descriptors (`frameRealm`, `frameKind`, `hasAxis`, `dimensionCount`, `hasMetricKind`,
  `determinacyModel`) and open values. Seed frames span terrestrial (WGS-84), indoor (Cartesian
  grid), celestial (ICRS, FK5, Galactic), virtual/network (IP/DNS topology), robotic (C-space,
  TF), mathematical/n-D (Hilbert, latent vector, phase space), biological-sequence (GRCh38),
  geocoding (Plus Codes, what3words), psychological/cognitive, and fictional/narrative realms.
  A new realm is *data*, never a schema change.
- **Time-scoped, contested, never a winner.** A place's name, jurisdiction, boundary, and
  parent containment are time-indexed and disputed. `JurisdictionTenure` and `ContainmentTenure`
  reify sovereignty and border changes as `gufo:SituationType` relators; contested claims
  (Crimea-class) coexist as standpoint-indexed instances, none privileged (Principle 9).
  Superseded places (Constantinople → Istanbul) are retained with `gmeow:displayable false`,
  never deleted (Principle 10).
- **Pose, motion, and trajectories.** A `gmeow:Pose` carries position + orientation as peers
  (quaternion, Euler angles, heading/bearing, or homogeneous matrix). `LocationState` captures
  position, velocity, and pose at an instant; `Trajectory` chains states into a space-time path.
  Interpolation and frame transforms live in the solver layer (Principle 12).
- **Topology, proximity, and aggregation.** RCC-8 relations (`rcc8po`, `rcc8tpp`, `rcc8ntpp`,
  `rcc8ec`, `rcc8dc`, `rcc8eq`) model qualitative spatial topology. `ProximityMeasurement`
  records frame-relative distance with an explicit `MetricKind` (geodesic, Euclidean, cosine,
  graph-hops). `SpatialAggregation` computes count, density, centroid, and k-anonymity over a
  region — all in the solver layer.
- **Cross-cutting facets.** Regulatory overlays (zoning, airspace, maritime zones, sanctions)
  bind a place, authority, regulation type, and optional 3D bounds. Accessibility features and
  barriers are orthogonal facets over places and routes. Privacy coarsening (`coarsenTo` +
  `GranularityLevel`) withholds or generalizes sensitive locations at projection time.
- **Maximal bridging, by reference.** Aligned to GeoSPARQL, BOT, CIDOC-CRM+CRMgeo, IVOA, UAT,
  SWEET, FALDO, Sequence Ontology, LADM, AIXM, UNCLOS, IEEE 1872-2015, OGC GeoPose, schema.org,
  vCard, WGS84, Wikidata — all by reference, never imported (Principle 5).

### Scientific & measurement utility

What began as a person-and-document vocabulary has, over the recent Location/Observation
epics, become a genuine **frame-relative observation and measurement ontology** — GMEOW can
now carry scientific data as first-class, attributed, frame-aware claims, not afterthoughts.
This is the fastest-growing edge of the project, and it composes cleanly with the
provenance/confidence/standpoint layer every other slice already uses:

- **Observation as a top-level claim-from-a-vantage.** `gmeow:Observation` (aligned to
  **SOSA/SSN** and **SensorThings**) makes every measurement an attributed claim with a
  result, a procedure, a time, and a vantage — so a sensor reading, a survey, and a model
  output are all first-class and comparable. Standpoint-indexed claims are themselves a
  *specialization* of observation (claim-from-a-vantage), unifying the epistemics spine.
- **Quantities carry their units and their uncertainty.** A universal `gmeow:Quantity` /
  `MeasuredValue` (value × unit × determinacy × provenance) aligns to **QUDT**, so "5 nm" and
  "5 µm" are never confused, and `SpatialMeasurement` + `CoordinateObservation` capture
  position *in an explicit reference frame*.
- **Frame-relativity is the law, not a convention ([Principle 11](./CONSTITUTION.md)).** Every
  coordinate, date, price, colour, or measurement is expressed against an explicit
  `gmeow:ReferenceFrame` (CRS, calendar + timescale, currency, colourspace, unit system),
  and heavy conversion is delegated to an external solver, never asserted into the logic
  ([Principle 12](./CONSTITUTION.md)). The reasoned graph stays decidable while the data stays
  honest about its frame.
- **Two orthogonal uncertainty axes.** Ontic **`gmeow:Determinacy`** (the thing itself is
  vague/indeterminate) is held apart from epistemic **`gmeow:confidence`** (how sure the
  recorder is) — a distinction scientific data needs and most vocabularies collapse.
- **Quality is measured, not assumed.** A data-quality layer aligned to **W3C DQV** and
  **ISO 19157** records completeness, accuracy, and lineage as structured, queryable claims.
- **Domain realms for real disciplines.** The reference-frame kernel now spans
  **astronomy** (celestial frames ICRS/FK5/Galactic; IVOA/UAT/SWEET), **genomics**
  (biological-sequence locations on GRCh38; FALDO/Sequence Ontology/GFF3, with liftover left
  to the solver), **robotics** (C-space, TF transform trees, SLAM occupancy grids;
  IEEE 1872-2015), and **mathematics/n-D** (Hilbert spaces, latent vectors, phase spaces;
  OpenMath/MEX) — each *data over the same kernel*, never a schema fork. Domain-specific
  observation surfaces for archaeology, astronomy, media, sensory environments, and research
  objects extend the same model rather than forking it.

## Publishing

GMEOW publication is generated from the graph and the release commit, not maintained as a
parallel metadata file.

1. **DOI/PID graph.** `gmeow-dev crossref` generates `dist/crossref-deposit.xml` (CrossRef
   deposit schema 5.4.0) from the canonical self-description for manual submission. The model is
   **single-anchor**: one concept DOI ([`10.67342/26w4o`](https://doi.org/10.67342/26w4o),
   always-latest) plus an optional per-release version DOI — granularity and provenance ride the
   content-addressed identifier triangle (`owl:versionIRI` ↔ SWHID / GTS head id /
   `gmeow:contentDigest`), not minted DOIs. The deposit maximally uses the schema (license,
   contributors + ORCID, institution, `hasFormat` relations to every serialization, and
   graph-projected alignment relations) and pairs with FAIR Signposting. See
   [`docs/dois.md`](./docs/dois.md).
2. **LOD and content negotiation.** `generated/metadata/void.ttl`, `generated/metadata/dcat.ttl`,
   and `generated/apache/gmeow.conf` are registered generated artifacts. The Apache config
   negotiates Turtle / RDF-XML / JSON-LD / HTML, handles profile and slice IRIs, and keeps
   release snapshots immutable ([Principle 6](./CONSTITUTION.md)).
3. **Verifiable packages and bundles.** PyPI wheels, npm/Cargo/Go release surfaces, signed
   GTS bundles, SBOMs, GitHub attestations, emoji verification fingerprints, and rsyncable GTS
   payloads are part of the publication contract.

## Current surface

The issue backlog is represented here as current capability:

- **Products.** The PyPI `gmeow`/`gmeow-gts` surface, grounded-memory MCP triad, claim spine,
  hallucination-resistant extraction pattern, eval leaderboard, GTS `ai-package`, signed
  verification, and multi-language GTS engines are the public adoption path.
- **Compliance-by-construction.** The generator registry, single `generated/` root, slice
  manifests, constitution-as-code, annotation-driven co-equal/suppression/frame guards,
  `owl:sameAs` hard gates, and RDF compliance report make constitutional drift a build failure.
- **Docs-with-the-ontology.** Every slice has a full guide; `gmeow describe` and `gmeow docs`
  work from the bundled GTS snapshot; `make docs` builds the native static site under
  `dist/ontology-docs`; the citation ledger lives in `metadata/references.ttl` and exports to
  CSL, BibTeX, Markdown, and generated docs.
- **Transpile and projection.** `gmeow transpile` lifts consumer RDF to a pure-GMEOW draft,
  then emits the MAXIMAL multi-vocabulary family with honest gap reports, real-data acceptance
  scores, consumer-clean language tags, single-vocabulary GTS views, and context-aware
  up-projection over graph position, structural inverses, SKOS identity, QID concept bridges,
  and polymorphic literal guards.
- **Logic.** The native `logic:` layer supplies typed IR, OWL/gUFO adapters, OWL/Datalog/N3/
  Prolog projections, Nemo materialization into world-indexed oxigraph named graphs, proof
  traces, profile certification, counterfactual revision, backward goals, and probabilistic
  weights with explicit preservation/loss ledgers.
- **Cognition and epistemics.** Objectual cognition (`isAwareOf` → `knowsAbout` →
  `understands` → `hasMastered`), mental moments, cognitive states, attention/interest/memory,
  propositional epistemics, doxastic state/tenure, credence, justification, defeaters, Gettier
  structure, and standpointed belief claims form the agent-memory mental-state spine.
- **Music and notation.** The music extension covers WEMI, pitch/tuning/time frames, structure,
  form/process/indeterminacy, performance participation, instruments/configurations, genre,
  oral tradition, timbre/sensory observations, notation projection profiles with declared loss,
  stress fixtures, and the GTS `music-package` render/import toolchain.
- **Domain breadth.** Email, calendars, organizations, employment, contacts, accounts, images,
  software provenance, research objects, finance, notes, genealogy, places, temporal data,
  accessibility, sensory environments, norms, risk, registers, rubrics, affect, narrative,
  archaeological evidence, language/lexicon/notation, rights, trust, attestation, evidence, and
  quality are modelled as slices with examples and projection/alignment surfaces.

## Licensing

GMEOW is **dual-licensed**. Blackcat Informatics® Inc. is the sole copyright holder
(© 2026) and makes the work available under open-source terms **and** reserves the right
to grant separate commercial/proprietary licenses.

- **Tooling code & Rust core** (this repository, excluding the vocabulary):
  [AGPL-3.0-only](./LICENSE).
- **GMEOW vocabulary** (the ontology in `ontology/`, the slices and mappings, and its
  published serializations) and the **documentation**: [CC BY 4.0](./LICENSE-ontology).
- **GTS engine.** The GTS format engine is a separate repository,
  [`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts), licensed
  Apache-2.0 OR MIT, and is not covered by the AGPL terms here.
- **Proprietary licensing.** The open licenses above are offered *in addition to* — not in
  place of — Blackcat Informatics®' right to license either part under separate commercial
  terms. Contact `licensing@blackcatinformatics.ca`.

**Trademarks.** "BLACKCAT INFORMATICS" (word mark, CIPO TMA1066935) and the
black-cat-silhouette & Sierpinski-triangle design mark (CIPO TMA1233860) are registered
trademarks of Blackcat Informatics® Inc.; "GMEOW" is not a trademark. Neither open license
grants any right to use these marks or logos (the AGPL-3.0 grants no trademark rights;
CC BY 4.0 §2(b)).

**Contributions** to tooling/code are accepted under AGPL-3.0-only, to the vocabulary and
docs under CC-BY-4.0, and to `gmeow-gts` under Apache-2.0 OR MIT — in each case, under the
project CLA, on terms permitting Blackcat Informatics® Inc. to relicense under separate
proprietary/commercial terms. See [`CONTRIBUTING.md`](./CONTRIBUTING.md).

**Third-party.** `imports/gufo.ttl` (gUFO) is vendored under the MIT License; its copyright
and permission notice are preserved in that file.

Full terms are in [`LICENSING.md`](./LICENSING.md); the propagating attribution and
trademark notice are in [`NOTICE`](./NOTICE).

## Conventions

`uv` for deps, `ruff` (format + lint) and `mypy --strict`, Google-style docstrings,
`pathlib.Path` everywhere, the Makefile as the canonical task runner. Missing required tools
fail loudly; the license guard and Wikidata validator error rather than silently degrade.

**AI and Agentic Development.** This ontology and its toolchain are developed and maintained with the assistance of AI coding agents (such as Google Antigravity and Claude Code). Workspace-specific rules and skills ([`AGENTS.md`](./AGENTS.md)) are defined to ensure agents strictly adhere to GMEOW's Constitution and compile pipelines.
