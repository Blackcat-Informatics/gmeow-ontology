<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The narrow waist: GTS is the only exit

Every projection of GMEOW data leaves the repository through one artifact:
**`generated/dist/gmeow.gts`** — the committed, byte-deterministic,
drift-gated GTS snapshot of the canonical sources.

```text
ontology/ + slices/          statements rdf12        SSSOM mappings
        \                         |                       /
         \                        |                      /
          +----------- gts generator (compile_gts) ----+
                              |
                    generated/dist/gmeow.gts
                    (default graph | gmeow:graph/statements
                     | gmeow:graph/alignments
                     | gmeow:graph/imports
                     | gmeow:graph/metadata)
                              |
            +---------+------+--------+----------+
            |         |               |          |
         exports   metadata        schemas      lpg
        (CSV/MD/  (VoID/DCAT)   (LinkML → 4   (Cypher/
        JSONL/                     targets)    GraphML/CSV)
        llms.txt)
```

## The rules

1. **One producer.** `compile_gts` is the only code that reads rdflib /
   gmeow_rdf sources for export purposes. It canonicalizes blank nodes,
   content-sorts the term table, and partitions sources into named graphs —
   the emitted bytes are a pure function of the inputs (cross-hash-seed
   tested).
2. **Many shims.** The four data exporters consume the fold through
   `gmeow_tools.gts_views.FoldView` and import **neither rdflib nor
   gmeow_rdf** (`metadata.py` keeps rdflib strictly as the *output
   serializer* for its freshly built description graphs — the one allowance).
3. **Sealed by Rust gate.** `crates/validate/src/repo_static.rs` runs through
   `make crate-check` and statically proves the exporter does not import RDF
   parsers or touch canonical-source loaders; it also keeps the public CLI from
   resurrecting retired GTS subcommands.
   Generator ordering remains registry-owned and outside this static seal:
   consumers must express dependencies through declared inputs — never by hand.
4. **Equivalence before deletion.** Each re-point proved value-equivalence
   against the old implementation before the old path was deleted —
   no compatibility shims survive.
5. **Committed without rebase pain.** `generated/dist/gmeow.gts` stays
   committed for reviewability and drift-gate visibility, but `.gitattributes`
   marks it `merge=ours -diff`. Running `make install` bootstraps the local
   Git merge driver (`merge.ours.driver=true`); after a merge or rebase,
   regenerate/check the bundle from canonical sources rather than resolving the
   binary file by hand.

## Box Roles In The Package

GTS keeps one transport waist, but the graphs inside that package serve
different roles:

| GTS surface | Box role | Meaning |
|---|---|---|
| default graph | TBox/RBox | The authored ontology vocabulary, properties, roles, and shape-visible annotations |
| `gmeow:graph/statements` | CBox | RDF 1.2 reifiers and statement annotations: provenance, confidence, time, standpoint, evidence, and related assertion context |
| `gmeow:graph/alignments` | TBox/RBox | Projection and mapping metadata that explains how GMEOW terms and properties bridge outward |
| `gmeow:graph/imports` | TBox/RBox | Vendored or extracted reference vocabularies used for reasoning and validation context |
| `gmeow:graph/metadata` | CBox/TBox | Package, citation, VoID/DCAT, and documentation metadata about the bundle itself |

These roles are descriptive metadata for package readers and validation
diagnostics. They do not change the narrow-waist rule: exports still consume the
verified GTS fold, never the canonical source tree directly.

## Why

Exporters that each re-read and re-interpret the sources drift from one
another; exporters that consume one verified fold cannot. The remaining
export tiers shipped this way: N-Quads/TriG, the statements JSONL
bundle, SKOS, OBO Graphs, and ShEx are emitters inside the sealed `exports`
generator, and Parquet is its own sealed `parquet` generator over the
`gts_db` relational schema — `GTS → *` shims over `gts_views`: no parser, no
drift surface, the same fold the published `gts`/`gmeow` packages read.
(OFN/OWX/OMN are release-tier ROBOT conversions in `gmeow build`; HDT was
refused — no maintained writer to pin.)

See [`GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md)
(in the `gmeow-gts` repo) for the format itself.
