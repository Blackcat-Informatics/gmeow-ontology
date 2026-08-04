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
gmeow.gts ──export──▶  dist/gmeow-okf/**.md  ──purrdf lift──▶ GMEOW (transpile)
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

## The seam: gmeow produces, purrdf lifts

The export lane's Rust generator (`crate::stages::okf`) is a direct structural
projection: it builds the GMEOW-specific bundle layout itself
(Class/Property/Individual docs, per-category indexes, relation links) and
hand-renders the Markdown/YAML-frontmatter bytes — it does not call a shared
codec, because the bundle shape (`okf:path`/`okf:body`/`okf:links` triples
scoped per rendered document) is synthesized procedurally from the folded
term surface, not carried natively by the RDF graph. The import lane is the
inverse direction only: `gmeow transpile <dir>` calls purrdf's native,
in-process OKF reader (`purrdf::lift_okf_bundle`) directly to fold a bundle
directory back to RDF, then lifts the recognized `okf:` predicates to GMEOW.
There is no external binary or subprocess in either direction — the former
`gts from-okf` seam is retired now that purrdf ships the codec directly.

The `okf:` contract (frontmatter keys): a **closed** vocabulary — exactly the
field set the export lane ever emits (`type`, `title`, `description`,
`resource`, `tags`, `version`, `curie`, `parents`, `prop_kind`, `domain`,
`range`, `functional`, `sub_property_of`, `types`, `alignments`,
`scope_notes`, `examples`, `use_when`, `avoid_when`, `how_to_use`,
`use_for_consumer`, `avoid_for_consumer`). `type` is a **string literal**
(`Class` / `Property` / `Individual`), not `rdf:type`; `resource` is the
subject IRI; the body's `[text](target)` links become reified `okf:links`
edges. purrdf's reader validates this profile strictly — an unrecognized
frontmatter key is a HARD FAIL, never a silently-accepted ad-hoc predicate.

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
gmeow transpile path/to/okf-bundle/ --profiles all
```

`gmeow transpile <dir>` detects an OKF directory, reads every `.md` file under
it into a purrdf `OkfBundle` in-process, folds it through purrdf's native
OKF reader (`purrdf::lift_okf_bundle`), then lifts the recognized `okf:`
predicates to GMEOW — `okf:title` → `rdfs:label`, `okf:description` →
`skos:definition`, `okf:type` → `rdf:type`, `okf:scope_notes` / `okf:examples`
→ the SKOS documentation predicates. Every **other** `okf:` triple is retained
verbatim as a provenance-bearing annotation (lossy honesty — never silently
dropped), and `MAXIMAL(G)` then runs over the draft exactly like the
[Turtle / YAML-LD transpile paths](./transpile.md).

There is no external binary dependency for this lane: purrdf ships the
OKF Markdown ↔ RDF codec directly, so `gmeow transpile` never shells out. A
malformed bundle (unsafe path, invalid YAML frontmatter, an unrecognized
frontmatter key, a dangling Markdown-link target) is a hard failure with no
degraded fallback.
