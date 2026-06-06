<p align="center">
  <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo — a black cat holding a linked knowledge graph" width="160" height="160">
</p>

# GMEOW — Global Metadata and Entity Ontology for the Web

GMEOW is a **reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary**
that unifies document metadata, entity descriptions, legal agreements, contacts and
person-centric data — a "super" FOAF + REL + DOAP + GEDCOM + PROV-O. It is grounded in
**gUFO** and aligned to FOAF, REL, DOAP, PROV-O, ORG, schema.org, OntoLex-Lemon and **Wikidata**.

**Why does this exist?** GMEOW unifies the sprawl of overlapping vocabularies used to
record a person's or organization's *digital existence*. See [`docs/RATIONALE.md`](./docs/RATIONALE.md)
for the reason, the problems it solves, and the solution it offers.

**Principles.** Every design decision and pull request is measured against
[`CONSTITUTION.md`](./CONSTITUTION.md) — twelve normative principles (RDF-1.2-first,
one-canonical-source, maximal bridging, greenfield, verified-by-construction, frame-relativity, …).
Cite them by number in issues and PRs.

- **Canonical IRI:** <https://blackcatinformatics.ca/gmeow> (slash namespace, term IRIs
  like `…/gmeow/Person`)
- **Vocabulary license:** [CC BY 4.0](./LICENSE-ontology) (dual-licensed — see [Licensing](#licensing))
- **Tooling license:** [Apache-2.0](./LICENSE) (dual-licensed — see [Licensing](#licensing))
- **Copyright:** © 2026 Blackcat Informatics® Inc.

**Three things GMEOW does that others don't:**

- **Statement-level provenance & confidence.** RDF 1.2 / RDF\*-first: every fact is an
  attributed, confidence-weighted, time-scoped claim, downcast losslessly to OWL axiom
  annotations for reasoners ([Principles 2–3](./CONSTITUTION.md); see *RDF 1.2* below).
- **Contested facts without a winner.** Disputed facts are recorded as coexisting,
  standpoint-indexed claims — never collapsed to a preferred, ranked, or latest value
  ([`docs/standpoints.md`](./docs/standpoints.md)).
- **Identity, naming & display safety.** Names and identity are reified, co-equal and
  self-asserted — no `primaryName`/`preferredGender`, deadnames suppressed-not-deleted, and a
  7-axis orthogonality matrix (pronouns ⟂ honorifics ⟂ gender identity ⟂ expression ⟂ sex
  ⟂ sexual ⟂ romantic orientation) **enforced by tests**
  ([`docs/names-mapping.md`](./docs/names-mapping.md), [`docs/identity-mapping.md`](./docs/identity-mapping.md)).

> **Status.** GMEOW is built **incrementally, one slice of digital existence at a time** — and
> the foundation is now broad. **24 modules** are modelled, aligned, and reasoned: identity
> (entities, names, gender, sexuality, languages), social & contact (genealogy, organization,
> contacts, email, accounts), content & evidence (documents, sources, software), trust & crypto
> (trust, messaging-trust), skills & legal (expertise, agreements, rights), place / time / events
> (places, temporal, events), and the epistemics spine (provenance, standpoint). The reasoning
> stack is in place — axiomatized doctrine (disjointness, relator mediation, kinship/containment
> property chains), the [OWL-infers / SHACL-validates split](./docs/reasoning.md) with
> entailment-based competency tests, and a gUFO↔BFO foundational bridge. Planned work is tracked
> as issues: new domain slices (calendar, notes, finance, employment, images, tagging, an
> AI / RAG claim-provenance layer), broad-consumption tooling (developer schemas, LPG / Croissant /
> RO-Crate exports, a maximal DOI strategy), and the current major thrust — the **Location as a
> universal reference-frame** epic, which surfaces cross-cutting foundations (observation as a
> *claim-from-a-vantage*, frame-relativity, determinacy ⟂ confidence, disclosure control by
> projection — one mechanism that **withholds *or* coarsens** a value under a trigger
> (suppression, privacy/consent redaction, and a universal **granularity / level-of-detail
> axis**), never by deletion — and the self-describing *Profile* meta-pattern) that the in-progress Constitution
> amendments (Principles 11–12) underpin. Each slice adds canonical terms, SSSOM alignment tables,
> projections, and a vendored fixture, and `make coverage` reports how much GMEOW covers and what
> gaps remain. The full toolchain (validate → reason → mappings → coverage → build → docs →
> publish) runs green at every step.

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
| `make verify` | Reasoned-graph SPARQL QC (ROBOT `verify` over `queries/verify/`) — the closed-world half of the [OWL-infers / SHACL-validates split](./docs/reasoning.md) |
| `make compile-mappings` | Compile the `mapping-dsl/` source → SSSOM + EDOAL + FnO + SPARQL artifacts (in-place) |
| `make mappings` | SSSOM → OWL/SKOS alignment axioms + VoID linksets; validates Wikidata QID syntax |
| `make wikidata` / `wikidata-live` | Wikidata QID/PID syntax gate (offline) / + existence check (network) |
| `make metadata` | Generate VoID (+ linksets) and DCAT dataset descriptions |
| `make crossref` | Generate the CrossRef DOI deposit XML (deposit schema 5.4.0) |
| `make apache` | Render the Apache content-negotiation include (`apache/gmeow.conf`) |
| `make docs` / `docs-full` | pyLODE HTML / + WIDOCO (diagrams, changelog, OOPS!) |
| `make build` | All serializations (`ttl`/`rdf`/`nt`/`jsonld`) + JSON-LD context + apache.conf → `dist/` |
| `make compile-statements` | Compile `statement-dsl/` → RDF 1.2 / RDF\* lead artifact + OWL-form downcast (verified, in-place) |
| `make rdf12` | Emit the RDF 1.2 / RDF\* lead artifact + its OWL-form downcast via Apache Jena (**required** — fails if absent) |
| `make quality` | OOPS! pitfall scan (network, best-effort) |
| `make release` | Reasoned closure (HermiT) + build + metadata + RDF 1.2 / RDF\* lead artifact + OWL-form downcast |

The Java tools (ROBOT, WIDOCO, Jena) run as **pinned Docker images** (see
`src/gmeow_tools/config.py`); `make pull-images` pre-pulls them. Containers run as the
invoking user, so generated files are never owned by root.

## Architecture

```text
ontology/gmeow.ttl        Root ontology: metadata + owl:imports (gUFO + modules)
ontology/modules/*.ttl    Module stubs, each class grounded in a gUFO category
imports/                  Vendored gUFO (+ future extracted subsets)
imports/targets/          Validation-only axiom snapshots (alignment linter + foundational bridge); NOT in the reasoned closure
catalog-v001.xml          Offline IRI→file resolution for ROBOT/Protégé
mapping-dsl/              The single authoring source for the alignment layer (compiled)
mapping-dsl/foundational/ gUFO↔BFO foundational-spine bridge (by reference) — see docs/foundational-bridging.md
statement-dsl/            The canonical RDF 1.2 / RDF* statement-metadata source (compiled)
shapes/gmeow-shapes.ttl   SHACL completeness shapes
queries/competency/, qc/  Competency questions + QC SPARQL
queries/rdf12-project.rq  Codec: OWL axiom-annotation downcast → RDF 1.2 / RDF* (compile step)
metadata/                 VoID + DCAT (generated)
apache/gmeow.conf         Content-negotiation include (generated)
src/gmeow_tools/          The toolchain (CLI: `gmeow …`)

# Generated by `gmeow compile-mappings` from mapping-dsl/ — do not hand-edit:
mappings/*.sssom.tsv      Cross-ontology alignments (SSSOM)
projections/*.edoal.ttl   Complex alignment cells (EDOAL) + functions.fno.ttl (FnO)
queries/projections/*.rq  Executable projection CONSTRUCTs (SPARQL)

# Generated by `gmeow compile-statements` from statement-dsl/ — do not hand-edit:
statements/gmeow.rdf12.ttl           RDF 1.2 / RDF* statement metadata (canonical lead)
statements/gmeow-statements.owl.ttl  OWL axiom-annotation downcast (merged into reasoning)
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
- **Foundational bridging (the spine reaches outward).** gUFO's *nature* categories are aligned
  by reference to **BFO 2020** (ISO/IEC 21838-2) — `skos:closeMatch`, never imported — so GMEOW
  interoperates with the OBO-Foundry / ISO top-level world. This is Principle 5 applied
  recursively to the foundational layer; the emitted BFO IRIs are verified against a vendored
  class snapshot (`imports/targets/bfo.ttl`), kept out of the reasoned closure. DOLCE/SUMO are
  planned next. Authoring source: `mapping-dsl/foundational/`; full guide:
  [`docs/foundational-bridging.md`](./docs/foundational-bridging.md).

### Linking & the license policy

Alignments are authored once in the **mapping DSL** (`mapping-dsl/`) and compiled
(`gmeow compile-mappings`) to SSSOM + EDOAL + FnO + SPARQL — see [§ The mapping
compiler](#the-mapping-compiler). Asserting a link (`owl:equivalentClass`,
`skos:exactMatch`, …) to any external term is always permitted — it copies nothing.
**Copying** axioms in (via `owl:imports` / ROBOT `extract`) is license-gated: a
reference-only source (NC/ND/share-alike/copyleft/proprietary) is **refused**
(`gmeow extract --target …`). The policy is classified by license family in
`config.py`, so new targets are classified correctly by default.

### The mapping compiler

GMEOW's doctrine — *one canonical source, everything else a generated lossy
projection* ([Principle 4](./CONSTITUTION.md)) — applies to the alignment layer
itself. Every mapping is authored
**once** as a `gmeow:`-grounded Turtle cell in `mapping-dsl/`, and `gmeow
compile-mappings` renders the four standard artifacts (SSSOM term links, EDOAL
complex cells, FnO transform functions, SPARQL CONSTRUCT executors). Drift is
impossible by construction; `gmeow compile-mappings --check` is the CI no-drift
gate. The compiler uses each target language to its full extent (EDOAL
`compose`/`inverse` relation paths, FnO `fnom` implementation linkage, SSSOM
provenance + labels, the full SPARQL path/expression algebra) — all expressed as
GMEOW vocabulary, never raw SPARQL. Full reference + authoring guide:
[`docs/projections.md`](./docs/projections.md).

### RDF 1.2 / RDF\* — the canonical statement-level model

GMEOW is **RDF 1.2 / RDF\*-first** ([Principles 2–3](./CONSTITUTION.md)): statement-level
metadata — provenance, confidence, temporal scope — is **authored once** as native RDF 1.2 /
RDF\* content in `statement-dsl/`, the canonical source. From it `gmeow compile-statements`
generates two verified artifacts: the **RDF 1.2 / RDF\* serialization** (the lead form, via
Apache Jena — the only engine that emits triple terms today) and the **OWL 2 axiom-annotation
form** (`owl:Axiom` + `owl:annotatedSource/Property/Target`) — the *generated,
reasoning-lossless downcast* that the OWL 2 DL reasoners GMEOW gates on actually consume. The
OWL form is the **downgrade for legacy tooling** — the same lossy-compatibility-as-projection
principle GMEOW applies to schema.org / vCard / FOAF ([Principle 4](./CONSTITUTION.md)), not a
competing source of truth — and it recedes as RDF-1.2-native reasoners and stores arrive. Both
downcasts are guarded by a no-drift `gmeow compile-statements --check`
([Principle 7](./CONSTITUTION.md)). The scope is exact: the **logical TBox stays OWL 2 DL** —
triple-terms are not OWL 2 DL, and GMEOW never claims otherwise.

### Names: first-class, multi-culture, inclusive

Most vocabularies treat a name as a flat string (`familyName`). GMEOW models it as a
**reified, time-bounded, context-scoped, source-attributed relationship** — a
`gmeow:Appellation` borne by an entity, with the `gmeow:NameUsage` relator binding *who is
named × which name × toward whom × in what register × over what period*. That makes naming
non-standard in deliberate, useful ways (full rationale in
[`docs/names-mapping.md`](./docs/names-mapping.md)):

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
[`docs/languages-mapping.md`](./docs/languages-mapping.md):

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
   dereferencing. Releases are **immutable** — fix issues in a new version, never in place
   ([Principle 6](./CONSTITUTION.md)).

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

**Trademarks.** "Blackcat Informatics®" is a registered trademark of Blackcat Informatics
Inc. Neither open license grants any right to use these names, logos, or marks (Apache-2.0 §6;
CC BY 4.0 §2(b)).

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

**AI and Agentic Development.** This ontology and its toolchain are developed and maintained with the assistance of AI coding agents (such as Google Antigravity and Claude Code). Workspace-specific rules and skills ([`AGENTS.md`](./AGENTS.md)) are defined to ensure agents strictly adhere to GMEOW's Constitution and compile pipelines.
