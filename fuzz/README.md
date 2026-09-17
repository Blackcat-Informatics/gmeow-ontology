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
those parsers and the three Common Logic dialects. Generic RDF, GTS, SHACL,
SSSOM and statement-codec checks belong to PurRDF. Its
`crates/rdf/tests/never_panic.rs` and `crates/shapes/tests/never_panic.rs`
own those parser contracts; GMEOW does not rerun them.

## Targets

| Target | Parser | Crate |
|---|---|---|
| `logic` | canonical RDF 1.2 `LogicProgram` frontend | gmeow-logic-compile |
| `query` | native `.logic` query-program parser | gmeow-logic |
| `clif` | Common Logic Interchange Format reader | gmeow-logic-compile |
| `cgif` | Conceptual Graph Interchange Format reader | gmeow-logic-compile |
| `xcl` | XML Common Logic reader | gmeow-logic-compile |

## Running

```bash
cargo install cargo-fuzz          # one-time
make fuzz-smoke                   # bounded run of every target (CI-friendly)
make fuzz-substrate-check        # read-only lockfile identity check
cargo fuzz run logic              # unbounded, single GMEOW target
cargo fuzz run logic fuzz/corpus/logic fuzz/seeds/logic     # seed from seeds/
```

`fuzz/seeds/<target>/` holds a small, committed **seed** corpus with representative
valid and near-valid inputs. The live working corpus `fuzz/corpus/` and crash
`artifacts/` are git-ignored. **Any crash artifact must become a regression seed
and the underlying panic must be fixed at its source.**

The scheduled lane in `.github/workflows/fuzz.yml` runs every GMEOW target with a
longer bounded budget. The manifests retain `purrdf = "2"`; the committed root
and fuzz lockfiles must select identical PurRDF package identities.
`make fuzz-smoke` verifies that identity before running any target.
