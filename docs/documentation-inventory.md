<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Documentation inventory and cleanup map

This inventory covers the authored documentation outside generated outputs. It is
intended as a migration map, not as a migration itself: no file should be moved,
deleted, or merged until its target is agreed and the canonical source is ready.

The goal is to make the documentation set smaller, more canonical, and more useful
to generated ontology docs:

- move slice doctrine and implementation rationale into `slices/*/*/design/`
  where the ontology-docs generator can surface it beside the slice;
- promote term-level guidance, examples, and interoperability caveats into the
  ontology itself where they can drive generated term pages;
- keep root `docs/` for project-level, release, publishing, and process material;
- flag stale paths, ticket-heavy notes, and duplicate doctrine before they spread.

## The documentation-maturity standard

The bar a slice's documentation is held to is not prose vigilance; it is a lattice of
structural coverage dimensions the docs generator detects and the maturity gate scores. The
`gmeow:DocMaturity` anchors (Minimal → Basic → Full → Maximal) and the
`gmeow:DocCoverageDimension` individuals are minted in
`slices/core/documentation/module.ttl`; the generator projects per-term coverage into the
`gmeow:graph/documentation` named graph and reds the build when a slice's asserted tier
exceeds the tier its coverage earns.

The **normative definition of FULL and MAXIMAL as exactly the surfaces an author must
provide** lives in the Slice Guide, [`SLICE_GUIDE.md`](SLICE_GUIDE.md) § 6.8 — that is where
authors decide what to write, so the required-dimension enumeration is stated there once and
pinned to the vocabulary by `crates/docs/tests/doctrine_matches_vocabulary.rs`. This
inventory does not restate the enumeration (a second copy would drift); it points to the
single source. The per-doc realized-state column that FULL requires is cross-referenced from
[`GROUNDING.md`](GROUNDING.md) § coverage duty.

## Generator readiness status

The docs generator already discovers `slices/*/*/design/*.md` and renders those
files as slice-local design pages. The generated Markdown and HTML site belongs
under `dist/ontology-docs/`, not in the committed tree. Documentation payloads
do not ride `gmeow.gts`; `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` discovers the canonical source set,
including `slices/*/*/design/*.md`, and regenerates every external projection.

## Classification keys

| Action | Meaning |
|---|---|
| `keep-root` | Keep as project-level documentation under `docs/`. |
| `move-to-slice-design` | Move into one or more `slices/*/*/design/` directories after review. |
| `merge-with-slice-docs` | Merge useful material into existing `slices/*/*/docs.md` or slice design docs, then retire the root duplicate. |
| `promote-to-ontology` | Extract term-level material into ontology annotations or mapping metadata. |
| `update-or-archive` | Refresh stale paths/history or archive after useful content is preserved. |
| `migrated` | Completed state: curated content extracted into slice annotations/docs; root doc retired. |
| `moved` | Completed state: relocated to another location/repository and referenced by URL. |

## Root docs inventory

| Path | Current role | Signals | Recommended action | Notes |
|---|---|---|---|---|
| `docs/BRAND.md` | Brand and visual identity | Ticket references; no ontology terms | `keep-root` | Project-level brand guidance. Keep out of generated ontology doctrine unless selected excerpts become site style copy. |
| `docs/CITATIONS.md` | Global citation and reference policy | Citation slice overlap; Principle 4 | `keep-root`, `promote-to-ontology` | Keep as global references policy. Extract durable citation modeling rules into `slices/core/citations` annotations or design notes. |
| `docs/CLI_ARCHITECTURE.md` | CLI & MCP architecture: the as-built crate record and three-surface (consumer `gmeow`, developer `gmeow-dev`, MCP) design of the native Rust CLI | Project-level tooling/DX doctrine; peer to `cli-extensions.md` and `PIPELINE_SPINE.md` | `keep-root` | Keep as the canonical CLI/MCP architecture reference; keep it in step with the shipped `cli-core` / `gmeow-cli` / `gmeow-dev-cli` crates. |
| `docs/GTS-SPEC.md` | Full GTS transport specification | Large normative spec; GTS slice references it | `moved` | Moved to the [`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) repo (`docs/GTS-SPEC.md`) alongside the four engines. The GTS slice references it by URL. |
| `docs/RATIONALE.md` | Project rationale | Many GMEOW terms; several principles | `keep-root`, `promote-to-ontology` | Keep as top-level "why GMEOW exists"; extract consumer-facing term guidance where it describes concrete modeling choices. |
| `docs/REALIGNMENT-v0.2.0.md` | Historical release specification | Many ticket references | `update-or-archive` | Preserve as release history or move under a release/archive area. Do not mix historical migration notes into generated slice docs. |
| `docs/TESTING.md` | Testing architecture: the declarative test-DSL, the native slice-test harness, and the competency-question reasoning model | Process/toolchain; test-DSL vocabulary references | `keep-root` | Keep as the testing reference. Update when the harness, the test-DSL cell types, or the competency reasoning lanes change. |
| `docs/cli-extensions.md` | CLI extension design | Slice CLI and packaging references | `update-or-archive` | Keep only if it describes current CLI behavior. If current, consider a design doc under a CLI/tooling area; otherwise archive as implementation history. |
| `docs/deception.md` | Older deception doctrine | Duplicates rich slice docs; stale paths | `merge-with-slice-docs` | `slices/core/deception/docs.md` appears newer and more complete. Preserve any missing examples/QID links, then retire root duplicate. |
| `docs/dublin-core.md` | Dublin Core alignment and projection guide | Rights/projection overlap; stale generated paths | `promote-to-ontology`, `move-to-slice-design` | Split between mapping DSL metadata and an interop/projection design note. Avoid leaving hand-maintained generated-file paths. |
| `docs/evidence-warrant-notability.md` | Evidence, warrant, and notability distinction | Strong evidence/quality doctrine | `migrated` | Migrated into the evidence slice (`slices/core/evidence/docs.md` orthogonality table + consumer-perspective notes, and `slices/core/evidence/module.ttl` term annotations incl. the `supportsNotability` WP:GNG-triad scope note); root doc retired. The quality slice owed nothing — the source was evidence-only. |
| `docs/foundational-bridging.md` | gUFO/BFO upper-ontology bridge | External-vocabulary doctrine; stale paths | `move-to-slice-design`, `promote-to-ontology` | Likely belongs near `slices/grounding/logic/design/` until the planned internal upper-ontology replacement exists. Extract consumer-facing explanations for gUFO/BFO terms. |
| `docs/git-provenance-boundary.md` | Git/source provenance boundary | Provenance, sources, temporal, software overlap | `move-to-slice-design`, `promote-to-ontology` | Likely target: `slices/core/provenance/design/`, with software-specific parts later moving to `slices/extensions/software/design/`. |
| `docs/gts-narrow-waist.md` | GTS architectural doctrine | GTS doctrine; ticket-heavy | `merge-with-slice-docs` | Fold into GTS design docs or GTS spec introduction. Avoid a separate root note after migration. |
| `docs/gts-reference.md` | Python GTS reference implementation note | GTS implementation detail | `moved` | Moved to the [`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) repo (`docs/gts-reference.md`). |
| `docs/hallucination-resistant-kg.md` | AI/GraphRAG pattern | AI, evidence, standpoint overlap | `move-to-slice-design`, `promote-to-ontology` | Likely split between `slices/core/ai/design/` and `slices/extensions/graphrag/design/`. Promote the modeling pattern into term usage annotations. |
| `docs/identity-mapping.md` | Gender/sexuality/names modeling guide | Identity terms and QIDs | `move-to-slice-design`, `promote-to-ontology` | Split across `slices/core/names`, `slices/core/gender`, and `slices/core/sexuality`. Promote term examples and use/avoid guidance. |
| `docs/import-provenance.md` | Import envelope and carrier-time doctrine | Provenance/sources/temporal terms | `move-to-slice-design`, `promote-to-ontology` | Likely target: `slices/core/provenance/design/IMPORT-ENVELOPE.md`, with source-carrier facts extracted to annotations. |
| `docs/key-management.md` | Release key management | Project release/process content | `keep-root` | Keep as process/security documentation. Do not generate into ontology docs unless a release-process section is introduced. |
| `docs/location-mapping.md` | Locations modeling and interoperability guide | Very high term density; many QIDs | `move-to-slice-design`, `promote-to-ontology` | High-value extraction candidate. Likely target: `slices/core/places/design/LOCATION-MAPPING.md`; promote frame, coordinate, geocode, and jurisdiction guidance into term annotations. |
| `docs/lpg-mapping.md` | RDF-to-LPG mapping specification | Projection/tooling design | `move-to-slice-design` | Likely cross-cutting projection design. Until a projection slice/area exists, consider `slices/grounding/logic/design/` or a future generated interop design area. |
| `docs/mcp-server.md` | GMEOW MCP server note | AI and tooling overlap | `update-or-archive` | Keep only if current. If still active, move implementation doctrine to AI/tooling design docs rather than slice ontology doctrine. |
| `docs/projections.md` | Projection architecture | High term density; stale generated paths | `promote-to-ontology`, `move-to-slice-design` | Split stable projection policy into design docs and promote profile-specific caveats into mapping DSL metadata. |
| `docs/projects-mapping.md` | Projects/software/provenance guide | Software, attestation, provenance overlap | `move-to-slice-design`, `promote-to-ontology` | Split across `slices/extensions/software`, `slices/core/provenance`, and `slices/core/attestation`. |
| `docs/prompts/claim-extraction-v1.md` | Claim-extraction prompt | AI/evidence prompt artifact | `update-or-archive` | Treat as a versioned prompt artifact, not ontology doctrine. Link from AI docs only if still supported. |
| `docs/reasoning.md` | OWL/SHACL reasoning doctrine | Logic/toolchain doctrine; stale paths | `move-to-slice-design`, `promote-to-ontology` | Likely target: `slices/grounding/logic/design/REASONING.md`. Extract concise reasoner-vs-validator explanations for docs generation. |
| `docs/research-objects.md` | Research-object export note | GraphRAG/export focus | `move-to-slice-design` | Likely target: `slices/extensions/graphrag/design/RESEARCH-OBJECTS.md` or GTS export design if it is mostly transport. |
| `docs/rust-gts-integration.md` | Rust/GTS integration and version policy | Project toolchain/process content; required gmeow-gts dep | `keep-root` | GTS integration reference. Why GTS is required, the nightly/MSRV float policy, the AGPL/Apache-MIT license boundary, and the `validate --gts` path. Keep as project-level toolchain reference. |
| `docs/rights.md` | Rights alignment and projection reference | High term density; stale paths | `move-to-slice-design`, `promote-to-ontology` | Likely target: `slices/core/rights/design/RIGHTS-INTEROP.md`. Promote rights action, license, copyright, and trademark guidance into annotations/mapping metadata. |
| `docs/schema-projections.md` | LinkML/schema projection note | Projection/tooling design | `move-to-slice-design` | Keep as cross-cutting projection design; ownership should be decided with `docs/projections.md`. |
| `docs/standpoints.md` | Standpoint alignment and projection reference | Duplicates slice doctrine; stale paths | `merge-with-slice-docs`, `promote-to-ontology` | Merge with `slices/core/standpoint/docs.md` or create `slices/core/standpoint/design/STANDPOINT-PROJECTIONS.md`; extract CRMinf/PROV/Web Annotation guidance. |
| `docs/superpowers/plans/2026-06-13-cargo-npm-release.md` | Release implementation plan | Process/history | `update-or-archive` | Keep as historical implementation plan only if the superpowers/plans convention remains active. |
| `docs/temporal-queries.md` | TQL query-language guide | Temporal query docs; stale paths | `move-to-slice-design`, `promote-to-ontology` | Likely target: `slices/core/temporal/design/TQL.md`. Link to slice-local query files and promote query-pattern examples. |
| `docs/transpile.md` | Consumer RDF to GMEOW to multi-vocab pipeline | Projection architecture | `move-to-slice-design` | Cross-cutting projection design. Pair with projection and schema projection cleanup. |
| `docs/up-projection-audit.md` | Historical up-projection audit | Issue-specific audit | `update-or-archive` | Archive or fold surviving requirements into projection design docs. |
| `docs/validation-thresholds.md` | The validation gate floors and ratchet rule | Process/toolchain contract; gate floors | `keep-root` | Keep as the single source of truth for the four blocking validation floors. Update whenever a floor is ratcheted. |
| `docs/i18n.md` | Compiled PO translation layer and translator workflow | Process/toolchain; i18n commands and gates | `keep-root` | Keep as the i18n workflow reference. Update when extract/merge/export commands or the PO layout change. |
| `docs/up-projection-gap-triage.md` | Historical up-projection triage plan | Issue-specific triage | `update-or-archive` | Archive or fold current gaps into projection design docs/tests. |
| `docs/wikidata-mapping.md` | Wikidata interoperability guide | QIDs/PIDs; stale paths | `promote-to-ontology`, `move-to-slice-design` | Promote QID/PID guidance and links into mapping DSL metadata. Keep remaining doctrine with coreference/projection design. |
| `docs/mentation-architecture.md` | Cross-cutting mentation architecture map | Mentation spine and child slices | `keep-root` | Project-level north-star document; slice-specific doctrine should live in `slices/core/<slice>/design/` once implemented. |

## Non-Markdown root assets

| Path | Current role | Recommended action | Notes |
|---|---|---|---|
| `docs/dctap/gmeow-dublin-core.csv` | Dublin Core profile data | `promote-to-ontology` or `move-to-slice-design` companion | Keep with the Dublin Core cleanup. If it is canonical data, document its owner and generation/validation path. |
| `docs/gmeow-logo.svg` | Brand asset | `keep-root` | Keep with brand docs. |
| `docs/social-preview.svg` | Brand/site asset | `keep-root` | Keep with brand/docs site publishing assets. |
| `docs/social-preview.png` | Brand/site asset | `keep-root` | Keep with brand/docs site publishing assets. |

## High-value ontology extraction candidates

Use existing annotation properties before adding new ontology. The first pass should
extract concise, term-scoped prose into:

- `skos:definition` for durable meaning;
- `skos:example` for short concrete examples;
- `gmeow:useWhen` for adoption guidance;
- `gmeow:howToUse` for modeling patterns;
- `gmeow:useForConsumer` for consumer-facing advice;
- `gmeow:avoidForConsumer` for common mistakes and false friends.

| Source docs | Extraction target | Why it matters |
|---|---|---|
| `docs/location-mapping.md` | `slices/core/places/module.ttl` and place mappings | Highest term density. The frame/coordinate/geocode/jurisdiction guidance should appear on generated term pages, not only in prose. |
| `docs/rights.md` and `docs/dublin-core.md` | `slices/core/rights/module.ttl` and mapping DSL rows | Rights vocabulary is adoption-facing and externally linked. Projection caveats should become mapping metadata where possible. |
| `docs/standpoints.md` and `docs/deception.md` | `slices/core/standpoint`, `slices/core/deception`, statement metadata docs | These explain contested facts, refutation, and no-winner semantics. They should drive generated examples and usage advice. |
| `docs/import-provenance.md`, `docs/git-provenance-boundary.md`, `docs/projects-mapping.md` | `slices/core/provenance`, `slices/core/sources`, `slices/core/attestation`, `slices/extensions/software` | The boundary between carrier provenance, source provenance, software provenance, and claim provenance is easy to misuse. Generated docs should teach it. |
| `docs/identity-mapping.md` | `slices/core/names`, `slices/core/gender`, `slices/core/sexuality` | Identity docs need co-equal, non-privileged modeling guidance directly on terms. |
| `docs/hallucination-resistant-kg.md`, `docs/research-objects.md`, `docs/mcp-server.md` | `slices/core/ai`, `slices/extensions/graphrag`, GTS/export docs | Useful educational patterns should be generated from ontology and slice metadata instead of hidden in root notes. |

(`docs/evidence-warrant-notability.md` has been migrated into the evidence slice and retired — see the inventory row above.)

## Suggested follow-up batches

1. **Generator readiness**
   - Keep generated Markdown and HTML under `dist/ontology-docs/`.
   - Keep documentation projections external to `gmeow.gts` and reproducible with `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`.
   - Add/adjust targeted docs tests as new design-doc patterns appear.

2. **Low-risk deduplication**
   - Merge `docs/deception.md` into `slices/core/deception/docs.md` or a new deception design doc.
   - Merge GTS narrow-waist/reference material into GTS design docs.
   - Archive or retire historical up-projection triage/audit notes after active requirements are copied.

3. **High-impact ontology annotation extraction**
   - Start with places, rights, standpoints, and import provenance.
   - Keep extraction small and term-scoped; avoid importing long prose into `module.ttl`.
   - Regenerate docs and review the generated term pages for clarity.

4. **Cross-cutting design ownership**
   - Decide whether projection/reasoning/interop docs should be slice-owned, owned by a future generated design area, or kept root-level.
   - Until that decision is made, do not scatter cross-cutting docs across unrelated slices.

## Stale-path and cleanup warnings

Several older docs mention paths or commands from earlier layouts, including
`ontology/modules`, `mapping-dsl`, direct generated projection paths, and older
compile commands. Treat those docs as unsafe to link from generated docs until
their paths are corrected or their content is migrated into current canonical
sources.

Ticket references are useful in commit history and PR discussion, but they make
generated educational docs read like an internal changelog. When migrating prose
into slice docs or ontology annotations, remove ticket-first framing unless the
ticket number is historically necessary.
