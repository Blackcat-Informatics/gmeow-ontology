<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Design: External documentation projection distribution

Issue 1491, follow-up to PR 1490. Cite principles by number (`Principle 4`) in
issues and PRs.

## Decision Summary

| Item | Choice |
|------|--------|
| Distribution model | **External projections + a content-addressed DCAT catalog**, published as release assets |
| Re-embed docs in `gmeow.gts` | **FORBIDDEN** (user directive, 2026-07-19) — enforced as a permanent contract invariant, not a default |
| Canonical schema | ONE distribution catalog authored as meta-level ontology content (`graph/distribution-catalog`), digest-free |
| Per-release digests | A separate release-time DCAT **instance** manifest under `dist/` — never in the carrier |
| Source-backed export | Preserved: `make sync SYNC_OUTPUTS=docs` → `sync_docs` renders all eight distributions |
| `zstd-rsyncable` level-12 | Preserved; positively asserted over the shipped bundle's real payload frames |
| Size budget | **None reintroduced** — honors 1404 ("size is not a gate that matters"); the forbidden-embed invariant is the gate |

The whole job of this issue is to reconcile two doctrines that pull in opposite
directions on documentation, and to gate the reconciliation so it cannot regress:

- **PIPELINE_SPINE §5 (superset law):** every committed artifact under `generated/`
  is byte-reconstructible from `gmeow.gts`.
- **documentation-inventory.md:139:** documentation *projections* are deliberately
  kept **external** to `gmeow.gts` and regenerated with `make sync SYNC_OUTPUTS=docs`.

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

**Regeneration cost is essentially equal across all three designs**, because all three
render the *same* bytes from the *same* one-pass pipeline; they differ only in
*packaging* (A writes files, B frames a sidecar snapshot, C folds into the carrier).
So the decision rests on size and architectural fit, not regen cost.

### Why A wins

- **Keeps the 93.2% bundle-size win** PR 1490 landed (149 MB → 10 MB). C throws it away;
  B keeps a separate 98 MB artifact that duplicates what the external tree already holds.
- **Doctrine-default (Principle 14):** the release path already publishes a
  content-addressed GitHub release with a checksum sidecar; A extends that pattern to the
  docs tarball + DCAT manifest, adding no new distribution machinery class.
- **No policy conflict:** A never re-embeds, so AC5's "size budget" precondition is never
  reached and no size gate is reintroduced (honoring 1404). C would require the
  1404-killed size budget.
- **Content-addressed, dogfooded:** each distribution carries a `blake3:<hex>` in a DCAT
  `spdx:checksumValue`, and the consumer verb `gmeow docs verify` checks it.

## Segmentation by consumer need (AC2)

Documentation is **not one indivisible payload**. It is two families of distributions,
each format serving a distinct repo-free consumer. This matrix is a *projection* of the
single canonical `graph/distribution-catalog` (Task 2) — it is not authored twice; it is
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
- **1404 (size gate removed):** honored — no size budget is reintroduced. Re-embedding is
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
  all eight distributions + the content-addressed DCAT release manifest.
- `gmeow-dev docs-package` + `make release-publish` wiring — the content-addressed release
  assets (docs tarball + manifest + `blake3` sidecars) beside the signed `.gts`.
- `gmeow docs matrix` / `gmeow docs verify` — the repo-free consumer verbs that query the
  shipped catalog and verify the manifest digests.
- `crates/gmeow-dev-cli/tests/docs_distribution_contract.rs` — the test-gated distribution
  contract enforcing every criterion above.
