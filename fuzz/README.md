<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# `gmeow-fuzz` — cargo-fuzz harness for the format frontends (T7, #788)

Deep, coverage-guided fuzzing of the **"reject malformed, never panic"** doctrine:
every parser given arbitrary bytes must return `Ok`/`Err`, never panic/abort.
libFuzzer aborts only on a panic, so a crash artifact is a contract violation.

This is the **nightly / on-demand deep** counterpart to the always-on
[`never_panic.rs`](../crates/rdf/tests/never_panic.rs) **proptest gate** that runs
in the normal `cargo nextest` CI lane. The proptest gate is the guaranteed,
portable contract enforcement; this crate explores far deeper given time.

## Targets

| Target | Parser | Crate |
|---|---|---|
| `nquads` | the lenient oxigraph `RdfParser` path `parse_quads` wraps (N-Quads/Turtle/TriG/N3) | gmeow-rdf |
| `gts` | `gts::read_graph` (GTS container, both segment modes) | gmeow-rdf |
| `shacl` | `engine::parse_shapes` (SHACL shapes) | gmeow-shacl |
| `sssom` | `sssom::parse_tsv` (SSSOM TSV) | gmeow-rdf |
| `statements` | `statements::{project_owl_to_rdf12, normalize_rdf12_to_owl}` (RDF-1.2 ↔ OWL) | gmeow-rdf |

The crate depends only on `gmeow-rdf` + `gmeow-shacl` (oxigraph-backed) so the
libFuzzer + sanitizer build never drags the heavy `nemo` tree. The **logic**
frontends (`parse_logic_str` / `parse_query_program`) are covered by the proptest
gate; their fuzz targets are deferred (nemo build cost). **CLIF/CGIF/XCL (#718)**
and **full-FOL IR (#719)** parsers do not exist yet — no targets until they land.

## Running

```bash
cargo install cargo-fuzz          # one-time
make fuzz-smoke                   # bounded run of every target (CI-friendly)
cargo fuzz run nquads             # unbounded, single target
cargo fuzz run nquads fuzz/corpus/nquads fuzz/seeds/nquads   # seed from seeds/
```

`fuzz/seeds/<target>/` holds a small, committed **seed** corpus drawn from the
real `conformance/` + slice inputs (the substitute for a `/vectors` dir). The
live working corpus `fuzz/corpus/` and crash `artifacts/` are git-ignored. **Any
crash artifact must be checked in as a regression seed and the underlying panic
fixed** (the contract is "never panic", so a third-party parser panic is hardened
at our entry point with `catch_unwind` → `Err`, our own panics are root-fixed).

A scheduled longer budget runs in `.github/workflows/fuzz.yml`.
