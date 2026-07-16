<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# OKF — the Open Knowledge Format agent surface

**OKF (Open Knowledge Format)** is a vendor-neutral, agent-facing knowledge
format: a directory of Markdown documents, one per concept, each with YAML
frontmatter and `[text](path)` links. AI agents consume it directly — no RDF/OWL
parser required. GMEOW projects its vocabulary *to* OKF (export) and lifts an
OKF bundle back *into* GMEOW (import), so the format is a fully **bidirectional**
surface.

```text
gmeow.gts ──export──▶  dist/gmeow-okf/**.md  ──gts from-okf──▶ GMEOW (transpile)
 (the fold)            (one doc per term)      (recognized okf: → rdfs/skos/rdf)
```

OKF is a **LOSSY down-projection** — the same doctrine slot as the
[SKOS / OBO Graphs / ShEx](./projections.md) views, and the deliberate opposite
of the lossless YAML-LD-star / JSON-LD-star sibling. Only the flat term surface
is carried (label, definition, the documentation advisories, and the IS-A /
domain / range / sub-property links); the OWL axioms, the RDF-star
statement/reification layer, and the full alignment graph stay in the canonical
grounding-slice sources carried by GTS. The lossiness is declared **in-band** in the bundle's root
`index.md`, mirroring how the OBO Graphs view declares its own loss.

## The seam: gmeow produces, `gts` validates

The Markdown ↔ graph codec is the **Rust `gts` primitive** (`gts from-okf` /
`gts to-okf`, built `--features okf`); GMEOW never re-implements it. The export
lane *produces* a bundle conformant to the `okf:` profile that `gts from-okf`
folds; the import lane shells `gts from-okf` to *consume* it. This is the
[gts/gmeow seam](./gts-narrow-waist.md) — `gts` owns format conversion, gmeow
owns the ontology projection and lift.

The `okf:` contract (frontmatter keys): the six recognized keys
`type` / `title` / `description` / `resource` / `tags` / `timestamp`, plus
arbitrary extension keys that fold to `okf:<key>`. `type` is a **string literal**
(`Class` / `Property` / `Individual`), not `rdf:type`; `resource` is the subject
IRI; the body's `[text](target)` links become reified `okf:links` edges.

## Export lane

```bash
gmeow okf --out dist/gmeow-okf            # one Markdown concept doc per term
gmeow okf --lang fr --out dist/gmeow-okf  # language-selected labels/definitions
```

Each term becomes `<category>/<curie-local>.md`: frontmatter carries the
recognized keys plus the structured `Term` fields (`curie`, `parents`,
`domain`/`range`, `prop_kind`, `scope_notes`, `examples`, …) as `okf:<key>`
extensions; the body carries the definition, the usage advisories, and a
**Relations** section with `[label](relpath)` links to in-bundle parents /
domain / range / sub-properties. Emission is byte-deterministic.

The bundle is folded into `gmeow.gts` as the `REP_OKF` blob (derived from the
fresh pass-1 fold, so it is drift-stable in a single regenerate cycle), and read
back repo-free via `gmeow_tools.bundle.bundled_okf()`. The MCP server exposes it
to agents at `gmeow://ontology/okf-index`.

## Import / lift lane

```bash
GMEOW_GTS_BIN=/path/to/gts gmeow transpile path/to/okf-bundle/ --profiles all
```

`gmeow transpile <dir>` detects an OKF directory, shells `gts from-okf` to fold
it, then lifts the recognized `okf:` predicates to GMEOW — `okf:title` →
`rdfs:label`, `okf:description` → `skos:definition`, `okf:type` → `rdf:type`,
`okf:scope_notes` / `okf:examples` → the SKOS documentation predicates. Every
**other** `okf:` triple is retained verbatim as a provenance-bearing annotation
(lossy honesty — never silently dropped), and `MAXIMAL(G)` then runs over the
draft exactly like the [Turtle / YAML-LD transpile paths](./transpile.md).

The `gts` binary with OKF support is a **required** Rust dependency for this
lane (the consumed Python `gts` package carries no OKF codec). It is located via
`$GMEOW_GTS_BIN` → `gts` on `PATH` → the sibling `gmeow-gts/rust/target/`. A
missing binary is a hard failure with a clear remedy — no degraded fallback.
Build it with `cargo build --release --features okf --bin gts` in the
`gmeow-gts` repo. The `gts from-okf` round-trip conformance tests therefore run
in a separate acceptance lane (set `GMEOW_GTS_BIN`), not the fast `make check`.
