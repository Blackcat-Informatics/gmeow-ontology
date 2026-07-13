<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# `gmeow-fuzz` — cargo-fuzz harness for format frontends

Deep, coverage-guided fuzzing of the **"reject malformed, never panic"** doctrine:
every parser given arbitrary bytes must return `Ok`/`Err`, never panic/abort.
libFuzzer aborts only on a panic, so a crash artifact is a contract violation.

This is the **nightly / on-demand deep** counterpart to the always-on
[`never_panic.rs`](../crates/logic/tests/never_panic.rs) **proptest gate** in the
normal Rust test lane. The property gate covers the canonical logic and query
frontends on every change; this harness applies coverage-guided mutation across
those parsers, the three Common Logic dialects, and the purrdf data formats.

## Targets

| Target | Parser | Crate |
|---|---|---|
| `nquads` | `parse_dataset` across N-Quads, Turtle, TriG, and N-Triples | purrdf |
| `gts` | `gts::read_graph` in single- and multi-segment modes | purrdf |
| `shacl` | `shapes::engine::parse_shapes` | purrdf |
| `sssom` | `sssom::parse_tsv` | purrdf |
| `statements` | RDF 1.2 ↔ OWL statement transforms | purrdf |
| `logic` | canonical RDF 1.2 `LogicProgram` frontend | gmeow-logic-compile |
| `query` | native `.logic` query-program parser | gmeow-logic |
| `clif` | Common Logic Interchange Format reader | gmeow-logic-compile |
| `cgif` | Conceptual Graph Interchange Format reader | gmeow-logic-compile |
| `xcl` | XML Common Logic reader | gmeow-logic-compile |

## Running

```bash
cargo install cargo-fuzz          # one-time
make fuzz-smoke                   # bounded run of every target (CI-friendly)
cargo fuzz run nquads             # unbounded, single target
cargo fuzz run nquads fuzz/corpus/nquads fuzz/seeds/nquads   # seed from seeds/
```

`fuzz/seeds/<target>/` holds a small, committed **seed** corpus with representative
valid and near-valid inputs. The live working corpus `fuzz/corpus/` and crash
`artifacts/` are git-ignored. **Any crash artifact must become a regression seed
and the underlying panic must be fixed at its source.**

The scheduled lane in `.github/workflows/fuzz.yml` runs every target with a
longer bounded budget.
