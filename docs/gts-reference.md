<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GTS reference implementation (`gmeow_tools.gts`)

A small, dependency-light reader/writer for the **Graph Transport Substrate** wire
format specified in [`GTS-SPEC.md`](./GTS-SPEC.md). This is the **baseline** tier
(issue #268, under epic #267): it validates the spec empirically and seeds the
narrow-waist `RDF 1.2 → GTS → *` toolchain.

## What it covers

- **CBOR append-only log** + the four-table RDF 1.2 fold (`terms` / `quads` /
  `reifies` / `annot`), content-addressed blobs, and `snapshot` folding (§7).
- **Integrity** — deterministic CBOR + per-frame BLAKE3 self-`id` and the `prev`
  content-id chain, with the header-genesis preimage rule (§5, §9.1).
- **Transform catalog** — `identity` / `gzip` / `zstd`; the capability model degrades
  an unknown codec or an `encrypt` codec (no keys in the baseline) to an **opaque
  node** rather than failing the read (§8, §7.6).
- **Robustness** — torn-append detection (§3), damaged-frame isolation, and the
  canonical diagnostics (§2.3): `TornAppendError`, `DamagedFrame`, `BrokenChain`,
  `UnknownCodec`, `MissingKey`, `ConflictingReifier`, `PositionConstraint`, …
- **`gts → nquads`** transform (§14).

## Not yet (follow-ups under #267)

COSE signing/encryption and the crypto opaque paths (§9.2–9.3); nested-GTS recursion
(§12.1); the `index`/MMR acceleration (§6.2); the `RDF 1.2 → GTS` producer and the
`gts → {duckdb,sqlite}` shims; the transport/packaging ontology vocabulary.

## Use

```python
from gmeow_tools.gts import Writer, Term, TermKind, read, to_nquads

w = Writer(profile="dist")
w.add_terms([
    Term(TermKind.IRI, "https://example.org/Cat"),
    Term(TermKind.IRI, "http://www.w3.org/2000/01/rdf-schema#label"),
    Term(TermKind.LITERAL, "Cat", lang="en"),
])
w.add_quads([(0, 1, 2, None)])
data = w.to_bytes()                      # the GTS file (bytes)

graph = read(data)                       # parse + verify chain + fold
print(to_nquads(graph))                  # <…/Cat> <…/label> "Cat"@en .
```

CLI:

```bash
gmeow gts info  file.gts          # frame/term/quad/blob counts + diagnostics
gmeow gts to-nq file.gts -o out.nq
```

## Conformance

`tests/test_gts.py` implements the non-COSE subset of the spec's §18 vectors
(minimal file, `zstd`/`gzip` frames, unknown-codec → opaque, damaged frame, torn
append, header hash, suppression, datatype defaulting, conflicting reifier, position
constraints, blank-node locality, inline blob, snapshot fold). A conformant reader of
the baseline profile is intentionally small — the point of the format.
