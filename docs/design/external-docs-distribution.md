<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Design: External documentation projection distribution

Reconciles the superset law (PIPELINE_SPINE §5) with the documentation-inventory
doctrine that keeps rendered documentation external to `gmeow.gts`, and extends the
prior bundle-size reduction (a 93.2% shrink, 149 MB → 10 MB) by keeping every
rendered documentation/serialization projection permanently out of the carrier.

## Decision Summary

| Item | Choice |
|------|--------|
| Distribution model | **External projections + a content-addressed DCAT catalog**, published as release assets |
| Re-embed docs in `gmeow.gts` | **FORBIDDEN** (user directive, 2026-07-19) — enforced as a permanent contract invariant, not a default |
| Canonical schema | ONE distribution catalog authored as meta-level ontology content (`graph/distribution-catalog`), digest-free |
| Per-release digests | A separate release-time DCAT **instance** manifest under `dist/` — never in the carrier |
| Source-backed export | Preserved: `make regen SYNC_OUTPUTS=docs` → `sync_docs` renders all nine distributions (four doc-render, four serialization, and the interactive-runtime console) |
| `zstd-rsyncable` level-12 | Preserved; positively asserted over the shipped bundle's real payload frames |
| Size budget | **None reintroduced** — a byte-count ceiling is not a gate that matters; the forbidden-embed invariant is the gate |

The whole job of this design is to reconcile two doctrines that pull in opposite
directions on documentation, and to gate the reconciliation so it cannot regress:

- **PIPELINE_SPINE §5 (superset law):** every committed artifact under `generated/`
  is byte-reconstructible from `gmeow.gts`.
- **documentation-inventory.md:139:** documentation *projections* are deliberately
  kept **external** to `gmeow.gts` and regenerated with `make regen SYNC_OUTPUTS=docs`.

The reconciling seam: rendered docs live in ephemeral `dist/`/`ontology-docs`, **not**
under `generated/`, so the superset law does not bind them. What ships *inside*
`gmeow.gts` is the KB-scale **distribution catalog schema** (which distributions exist,
their family, their consumer class, their declared loss) — never the ~140 MB rendered
payload, and never a render-derived digest.

## The three designs, measured

All byte counts are produced by `gmeow-dev docs-measure` (a reproducible production
surface: it renders every format through the production renderers and frames each blob
through the single mandated `zstd-rsyncable` level-12 profile). Re-run the command to
reproduce these numbers from unchanged sources; the byte sizes are asserted
deterministic by the test-gated contract.

### Per-format footprint

| Format | Family | Uncompressed (B) | zstd-rsyncable L12 (B) |
|--------|--------|-----------------:|-----------------------:|
| site | doc-render | 482,824,388 | 38,682,860 |
| mdbook | doc-render | 33,524,529 | 5,322,733 |
| pdf (print) | doc-render | 14,169,354 | 8,731,545 |
| snippets | doc-render | 4,689,161 | 1,420,289 |
| pydantic | serialization | 2,614,902 | 197,030 |
| okf | serialization | 11,113,303 | 2,079,989 |
| jsonld / yaml-ld | serialization | 745,107,513 | 41,481,437 |

### Design totals

| Design | Total (B) | What it costs | Verdict |
|--------|----------:|---------------|---------|
| **A — external + DCAT manifest** (ADOPTED) | 1,294,047,246 uncompressed on disk (`dist/`) + a KB-scale manifest | zero bundle impact: `gmeow.gts` stays ~10 MB | **Chosen** |
| **B — sidecar `.gts`** | 97,916,216 | a second signed `.gts` artifact + a second emit path | Viable but strictly more machinery than A for no gain |
| **C — opt-in embedded profile** | 121,188,849 (analytical proxy: without-docs carrier + Σ framed doc bytes) | re-inflates the carrier ~12× (10 MB → ~121 MB) | **Rejected + forbidden** |

**Regeneration cost is equal across all three designs BY CONSTRUCTION, not by
measurement.** All three designs render the IDENTICAL bytes (the per-format footprint
table above) through the IDENTICAL single one-pass pipeline (`gmeow-dev
docs-measure`/`sync_docs` run the SAME production renderers once, in memory); they
differ only in what happens to those bytes AFTER rendering — *packaging*, not
*production* — A writes them to files, B frames them into a sidecar `.gts` snapshot, C
folds them into the carrier. Since none of the three designs adds, removes, or
reorders a pipeline stage, none can change how long rendering takes; the only
degrees of freedom are the constant-time byte-copy dressings each design's packaging
step performs. A wall-clock benchmark of the three would therefore measure
machine load and I/O-cache noise, not the designs — a non-deterministic, non-gateable
number the test-gated contract could never pin. The structural argument above (same
pipeline, same bytes, packaging-only divergence) is the correct and only form of
"measured regeneration cost" for this decision; it is what `gmeow-dev docs-measure`
actually measures, and it is what the byte totals in the table above are evidence of.
So the decision rests on size and architectural fit, not regen cost.

### Why A wins

- **Keeps the prior 93.2% bundle-size win** (149 MB → 10 MB). C throws it away; B keeps
  a separate 98 MB artifact that duplicates what the external tree already holds.
- **Doctrine-default (Principle 14):** the release path already publishes a
  content-addressed GitHub release with a checksum sidecar; A extends that pattern to the
  docs tarball + DCAT manifest, adding no new distribution machinery class.
- **No policy conflict:** A never re-embeds, so AC5's "size budget" precondition is never
  reached and no size gate is reintroduced (the size-gate-removal doctrine holds). C
  would require reintroducing the size budget that doctrine already killed.
- **Content-addressed, dogfooded:** each distribution carries a `blake3:<hex>` in a DCAT
  `spdx:checksumValue`, and the consumer verb `gmeow docs verify` checks it.

## Segmentation by consumer need (AC2)

Documentation is **not one indivisible payload**. It is two families of distributions,
each format serving a distinct repo-free consumer. This matrix is a *projection* of the
single canonical `graph/distribution-catalog` — it is not authored twice; it is
the result of `gmeow docs matrix` over the shipped bundle.

| Format | Family | Consumer class | Media type | Why the consumer needs it |
|--------|--------|----------------|------------|---------------------------|
| site | doc-render | `consumerPublicSite` | text/html | Repo-free browser reader; the reasoned SPARQL playground surface |
| mdbook | doc-render | `consumerOfflineBook` | text/markdown | Offline, downloadable book reader |
| pdf | doc-render | `consumerPrintArchive` | application/pdf | Print / archival reader (byte-reproducible Typst) |
| snippets | doc-render | `consumerAgentMemory` | text/markdown | LLM / agent prompt and retrieval ingestion |
| pydantic | serialization | `consumerTypedModelClient` | text/x-python | Typed-model client for code / data validation |
| okf | serialization | `consumerKnowledgeFederation` | application/json | Open-Knowledge federation interchange |
| jsonld | serialization | `consumerLinkedDataTooling` | application/ld+json | Linked-data / RDF tooling |
| yamlld | serialization | `consumerLinkedDataTooling` | application/yaml | Human-readable linked-data tooling |

The **doc-render** family is exactly the `gmeow_docs::formats::DocFormat` loss-lattice
(`site ⊆ mdbook ⊆ pdf ⊆ snippets`); each format's declared capability loss is read from
the single `format_capabilities` authority and drift-gated against the catalog. The
**serialization** family (okf, jsonld, yamlld, pydantic) is structured re-serialization,
not lossy prose rendering, and carries no fabricated lattice.

## Schema vs. instance — the load-bearing split

Documentation is rendered *from* `gmeow.gts`. Therefore a per-release digest placed
*inside* the carrier would be a non-converging fixpoint (the digest of THIS bundle would
have to be known before THIS bundle is serialized). The design splits accordingly:

- **Schema (environment-independent):** the `graph/distribution-catalog` meta-level graph —
  which distributions exist, families, consumer classes, declared losses, media types.
  Ships inside `gmeow.gts` as a **non-object-level** graph (out of reasoning closure),
  **digest-free**. Grounds Principle 4: one canonical home for the distribution schema.
- **Instance (per release):** the DCAT manifest `dist/gmeow-docs/manifest/docs-manifest.ttl`
  — one `dcat:Distribution` per format with `spdx:checksumValue = blake3:<hex>` + byte size,
  projecting the schema via the bundled `dcat.rq`. Lives only in `dist/`, never the carrier.

## Principle & doctrine reconciliation

- **Principle 4 (one canonical source):** the distribution schema + consumer-need matrix
  have exactly one authored home (the catalog); the DCAT manifest, the design-doc matrix,
  and `gmeow docs matrix` all project from it. Doc-render loss keeps its one home
  (`format_capabilities`) and is referenced, not re-authored.
- **PIPELINE_SPINE §5 (superset law):** satisfied — rendered docs are under `dist/`, not
  `generated/`, so they are unbound by the law; the catalog *schema* that does ship is
  reconstructible as a named graph of `gmeow.gts`.
- **Narrow-waist Rule 6:** `zstd-rsyncable` level-12 binds GTS frames only. The release
  docs tarball is a USTAR file, not a GTS frame; it carries a `blake3` sidecar. Any GTS
  frame authored routes through `gts_profile::emit_gmeow_gts`, and the contract positively
  validates every payload frame of the shipped bundle.
- **Size-gate removal stays honored:** no size budget is reintroduced. Re-embedding is
  forbidden outright, so the AC5 "budget-or-contract" precondition never triggers; the
  test-gated **forbidden-embed** invariant (`documentation_projections_are_absent` +
  "no `TOTAL_CEILING`") is the gate.
- **Principle 6/14 (immutable, content-addressed releases):** the release fold writes only
  `dist/`; the committed `generated/dist/gmeow.gts` is never mutated. The docs tarball +
  manifest join the signed `.gts` + checksum sidecar as content-addressed release assets.

## What ships

- `crates/pipeline/src/docs_measure.rs` + `gmeow-dev docs-measure` — the measured comparison.
- `crates/pipeline/src/stages/distribution_catalog.rs` + `graph/distribution-catalog` — the
  canonical schema, folded into `gmeow.gts`, digest-free.
- `crates/pipeline/src/docs_distribution.rs` + extended `sync_docs` — full external render of
  all nine distributions + the content-addressed DCAT release manifest. The nine are
  declared ONCE, in `distribution_catalog::DISTRIBUTIONS`; `sync_docs` iterates that table
  for its destinations and release rows rather than restating them.
- `gmeow-dev docs-package` + `make release-publish` wiring — the content-addressed release
  assets (docs tarball + manifest + `blake3` sidecars) beside the signed `.gts`.
- `gmeow docs matrix` / `gmeow docs verify` — the repo-free consumer verbs that query the
  shipped catalog and verify the manifest digests.
- `crates/gmeow-dev-cli/tests/docs_distribution_contract.rs` — the test-gated distribution
  contract enforcing every criterion above.

## npm distribution — the six published packages

The release-asset channel above distributes **rendered documentation**. A second,
disjoint channel distributes the **executable surfaces**: six scoped npm packages, all at
the workspace version, all published from the `v*` tag by `.github/workflows/release.yml`
*after* the native≡wasm parity lanes pass.

The split is by kind, not by taste. Five packages are **engines** — a `wasm32` image plus
the thin ESM shim that adds the one-time async instantiation the synchronous wasm boundary
cannot express. The sixth is an **application**: the console element, together with the
whole runtime it boots over — the browser transport, both MCP images and the `gmeow.gts`
snapshot — staged into its `pkg/` at pack time out of `gmeow-dev console-assemble`. It is
self-contained on purpose. An element published without that runtime is an element that
cannot start: its worker's relative import walks out of the installed package and 404s, and
the reader is told the engine worker failed to load.

| Package | Kind | What it is |
|---|---|---|
| `@blackcatinformatics/gmeow-validate-wasm` | engine | Tier-1 conformance (SHACL + OntoUML disciplines) over a `gmeow.gts` bundle, plus the GMN-1 codec validator against an embedded codebook |
| `@blackcatinformatics/gmeow-reason-wasm` | engine | the native structured-DL chase (reasoned closure) and the symmetric conjecture engine |
| `@blackcatinformatics/gmeow-gmn-wasm` | engine | the GMN-0↔GMN-1 codec and its glyph legend |
| `@blackcatinformatics/gmeow-mcp-core-wasm` | engine | the LEAN first-load MCP engine: the whole tool surface, reasoning segment demand-loaded |
| `@blackcatinformatics/gmeow-mcp-wasm` | engine | the demand-loaded REASONING segment the lean core dispatches into |
| `@blackcatinformatics/gmeow-console` | application | `<gmeow-console>` — the standalone console as one custom element, its DOM-free session module, and the engine payload it boots over |

**Why exactly these.** The set is not curated; it is the set of surfaces that already ship
a `js/` ESM shim over a `wasm-bindgen` build (the five `*-wasm-pkg` Make lanes) plus the
one browser application. Nothing else in the tree is consumable off the repository:
`crates/docs/assets/mcp{,-core}/` are re-vendored copies of two of the packages above (the
documentation site and the console's site tree share one 7 MB image rather than carrying a
second; the console's own npm tarball stages that same image, because an installed package
has no site around it to share with), `crates/docs/assets/console/smoke/` is a dev-only
Playwright manifest that declares itself `private`, and `editors/vscode/` is a Visual Studio
Marketplace extension published by `vsce`, on that registry's cadence and metadata contract,
not to npm.

A third vendored tree, `crates/docs/assets/purrdf/`, used to sit alongside those two: an
upstream package this repository does not author, vendored so the site's SPARQL playground
and bundle explorer could run offline. It is retired — both surfaces query the shipped
bundle through the MCP segments above, so the site carries one engine instead of two.

The package set is **discovered from the shipped bytes** — every `package.json` that is
neither `"private": true` nor a VS Code extension manifest — by
`scripts/npm-packaging.mjs` and, independently, by
`crates/gmeow-dev-cli/tests/npm_packaging_contract.rs`. There is no list of names in any
source file, so a package cannot be added without every gate below quantifying over it.

### CDN templates

Both public npm CDNs serve these packages directly. Pin the version: an unpinned specifier
resolves to whatever is latest at fetch time, which is a moving engine underneath a
reasoned result.

| Package | jsDelivr | unpkg |
|---|---|---|
| console element | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-console@0.2.0/element.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-console@0.2.0/element.mjs` |
| Tier-1 validator | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-validate-wasm@0.2.0/index.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-validate-wasm@0.2.0/index.mjs` |
| DL reasoner | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-reason-wasm@0.2.0/index.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-reason-wasm@0.2.0/index.mjs` |
| GMN codec | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-gmn-wasm@0.2.0/index.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-gmn-wasm@0.2.0/index.mjs` |
| MCP lean core | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-mcp-core-wasm@0.2.0/index.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-mcp-core-wasm@0.2.0/index.mjs` |
| MCP reasoning segment | `https://cdn.jsdelivr.net/npm/@blackcatinformatics/gmeow-mcp-wasm@0.2.0/index.mjs` | `https://unpkg.com/@blackcatinformatics/gmeow-mcp-wasm@0.2.0/index.mjs` |

This table is **drift-gated**: `cdn_documentation_names_exactly_the_published_packages`
parses the names out of these URLs and requires them to be exactly the discovered package
set, at exactly the workspace version. A package added, renamed, or version-bumped
without editing this table is a hard failure, not a stale doc.

### No runtime CDN loading

**No shipped surface fetches code from a CDN at runtime.** The URLs above are an
*install-time* convenience for a hand-written page; nothing this repository produces
contains one.

The reason is the offline contract, not preference. The console is a cache-first PWA whose
service-worker `SHELL` is generated from the assembled tree's FIRST-LOAD tier — the
demand-loaded reasoning segment is deliberately excluded, and is cached only once a pane
asks for it — and pre-cached with `cache.addAll`. A member that lives on a third-party
origin would be an install that
fails, or worse, an offline console whose engine silently is not there. It is also an
integrity contract: the vendored engine images are pinned by a BLAKE3 manifest and their
digests are verified against the bytes that shipped, which a third-party origin can
neither offer nor be held to. And it is a licensing/provenance contract: the release
publishes with `--provenance`, so the trustworthy artifact is the one the consumer
installed, not one a CDN resolved for them later.

Concretely: `gmeow-dev console-assemble` emits a self-contained tree in which every module
the console imports is a same-origin relative path, and the documentation site's
`assets/` tree carries its engines the same way. A consumer who wants the CDN form must
place the sibling `assets/` engine tree themselves, or point the transport at their own
origin with `configure({ assetBase })`.

### Gates

| Lane | What it proves |
|---|---|
| `crates/gmeow-dev-cli/tests/npm_packaging_contract.rs` | the discovered set is scoped, versioned, typed; export sets agree; Playwright is dev-only; the release workflow publishes fail-closed with parity FIRST; this table cannot drift |
| `crates/*/tests/npm_package_version.rs`, `crates/docs/tests/console_npm_package.rs` | each manifest's `version` equals its own crate's `CARGO_PKG_VERSION` |
| `crates/*/js/tests/exports.test.mjs` | export-set equality against the GENERATED `wasm-bindgen` `.d.ts` |
| `make npm-publish-dry` | every manifest packs cleanly, with no registry contact |
| `make npm-consumable` | every package installs from its own tarball and its witness passes against the INSTALLED bytes |
