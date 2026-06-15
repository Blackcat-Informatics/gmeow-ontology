<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The narrow waist: GTS is the only exit

Every projection of GMEOW data leaves the repository through one artifact:
**`generated/dist/gmeow.gts`** — the committed, byte-deterministic,
drift-gated GTS snapshot of the canonical sources (#267, #12).

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
   pyoxigraph sources for export purposes. It canonicalizes blank nodes,
   content-sorts the term table, and partitions sources into named graphs —
   the emitted bytes are a pure function of the inputs (cross-hash-seed
   tested).
2. **Many shims.** The four data exporters consume the fold through
   `gmeow_tools.gts_views.FoldView` and import **neither rdflib nor
   pyoxigraph** (`metadata.py` keeps rdflib strictly as the *output
   serializer* for its freshly built description graphs — the one allowance).
3. **Sealed by test.** `tests/test_narrow_waist.py` enforces it twice over:
   a behavioral seal (every canonical-source reader monkeypatched to raise;
   all five exporters must still render) and a static import seal (AST scan
   of the exporter modules). The registry orders `gts` before every consumer
   from declared inputs — never by hand.
4. **Equivalence before deletion.** Each re-point (PRs #370/#371/#373/#374)
   proved value-equivalence against the old implementation inside its own
   PR before the old path was deleted — no compatibility shims survive.

## Why

Exporters that each re-read and re-interpret the sources drift from one
another; exporters that consume one verified fold cannot. #12's remaining
tiers shipped exactly this way (#377): N-Quads/TriG, the statements JSONL
bundle, SKOS, OBO Graphs, and ShEx are emitters inside the sealed `exports`
generator, and Parquet is its own sealed `parquet` generator over the
`gts_db` relational schema — `GTS → *` shims over `gts_views`: no parser, no
drift surface, the same fold the published `gts`/`gmeow` packages read.
(OFN/OWX/OMN are release-tier ROBOT conversions in `gmeow build`; HDT was
refused — no maintained writer to pin.)

See `docs/GTS-SPEC.md` for the format itself.
