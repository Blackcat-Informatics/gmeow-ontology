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
- **`RDF → GTS` producer** — interns an rdflib `Graph`/`Dataset` (RDF 1.1 base graph)
  **and** an RDF 1.2 artifact read via pyoxigraph (the statement layer:
  `rdf:reifies` triple-terms → `reifies`, annotations → `annot`) into one dictionary.
  `gmeow-dev gts compile` builds a **statement-complete** `dist/gmeow.gts` from the
  merged ontology + `statements/gmeow.rdf12.ttl` (#271).
- **Transforms out** — `gts → nquads` (§14) and `gts → {sqlite,duckdb}` (the
  integer-id, dictionary-encoded relational load; the engine resolves ids via join).
- **COSE signing (§9.2)** — `Writer(signer=…)` COSE_Sign1-signs each frame over its
  `id` (EdDSA/Ed25519); `read(data, keys=…)` verifies and records per-frame status
  in `Graph.signatures` (`valid`/`invalid`/`unverified` under a `KeyProvider`). Plus
  **truncation detection** via `read(data, expected_head=…)` → `TruncatedLog` (#272).
- **COSE encryption (§9.3)** — `Writer.add_frame(…, encrypt=(kid, key))` seals a
  payload as `COSE_Encrypt0` (the outermost transform) and records the recipient;
  `read(data, keys=…)` decrypts when the content key is held, else the frame folds to
  a `missing-key` **opaque node** with its recipient visible (the opacity invariant) —
  selective disclosure (#272).

## Not yet (follow-ups under #267)

Multi-recipient / ECDH key-wrap (this lands single-recipient `COSE_Encrypt0`);
`evidence`/`opaque` profile *conformance enforcement* (signatures-required,
pseudonymous-`kid`); nested-GTS recursion (§12.1); the `index`/MMR acceleration
(§6.2); a frame-streaming DB load for very large inputs; the packaging vocabulary.

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
gmeow-dev gts compile -o dist/gmeow.gts # checkout merged ontology -> GTS snapshot
gmeow gts from-rdf data.ttl -o data.gts # consumer-safe arbitrary RDF -> GTS
gmeow gts info     file.gts             # frame/term/quad/blob counts + diagnostics
gmeow gts to-nq    file.gts -o out.nq
gmeow gts to-sqlite file.gts -o out.sqlite
gmeow gts to-duckdb file.gts -o out.duckdb
```

## Conformance

`tests/test_gts.py` implements the non-COSE subset of the spec's §18 vectors
(minimal file, `zstd`/`gzip` frames, unknown-codec → opaque, damaged frame, torn
append, header hash, suppression, datatype defaulting, conflicting reifier, position
constraints, blank-node locality, inline blob, snapshot fold). A conformant reader of
the baseline profile is intentionally small — the point of the format.
