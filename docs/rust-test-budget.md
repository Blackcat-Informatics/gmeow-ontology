<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Rust test-suite duration budget

The always-on gate enforces a **25 s per-test wall-time budget**. This note records
the measurement that set it, the mechanism, and the open follow-ups. The doctrine
(how to comply) lives in [`AGENTS.md`](../AGENTS.md) under *The 25 s per-test budget*.

## Mechanism

- **Enforcer:** `crates/test-budget` (`gmeow-test-budget`), a std-only leaf binary
  that parses `target/nextest/<profile>/junit.xml` and exits non-zero if any
  `<testcase>` exceeds the budget (`GMEOW_TEST_BUDGET_SECS`, default 25). Wired into
  `make rust-test` / `make rust-gate` and each CI rust shard, after
  `cargo nextest run --profile ci`.
- **Off-gate carve-out:** `.config/nextest.toml` `profile.default.default-filter`
  excludes the irreducibly-heavy tests from the gate; `profile.maint-heavy`
  (`make maint-rust-heavy`) re-includes everything. The filter expression is the
  single source of truth for the off-gate allowlist.
- The budget is **per-test, post-fixture**. A once-per-run setup cost (e.g. the
  shared docs-model build, primed before the run) is amortized across tests and
  intentionally not charged to any single `<testcase>`.

## Measurement snapshot (2026-06-26, 32-core local, debug)

Full `cargo nextest run --profile ci`: 2409 tests, ~554 s wall / ~3703 s summed.
**59 tests exceeded 25 s** before this work, in four shapes:

| Cluster | Count >25 s | Cause | Disposition |
|---|---|---|---|
| `gmeow-docs` render/competency/extract/lint/i18n/model | ~39 | each test rebuilt the full `DocsModel` via `discover` (~13 s) + render (~5 s), uncached across nextest's process-per-test model | **now on-gate** via a shared once-per-run model fixture (slowest ~12 s) |
| `gmeow-pipeline` full-fold / full-DAG / codec / mapping-parity | ~13 | whole-bundle fold + byte-parity | off-gate (bundle-size bound) |
| `gmeow-logic::ontology_entailments`, `gmeow-conformance` | ~3 (+ ~16 in 10–25 s) | native OWL-2-RL closures / chase | off-gate whole binaries |
| `gmeow-slice` / `gmeow-slicetest` stragglers | ~3 | whole-ontology emit/closure just over budget | off-gate the specific tests |

### The corpus-parity case

`crates/rdf/tests/sparql_eval_parity.rs::corpus_parity_against_real_ontology` ran
~50 s. Profiling the phases:

```text
gts_read 2.6 ms · oxigraph store 1.107 s · native dataset 338 ms · eval loop 48.8 s
```

The load is ~1.5 s — the eval loop dominated, and within it a **single** query,
`queries/verify/class-without-stereotype.rq` (a `FILTER NOT EXISTS` anti-join over
every `owl:Class`), took **~44 s on the native engine vs ~5 ms on oxigraph**. Fix:
carve that one query into the off-gate `corpus_parity_heavy_offgate` test and shard
the rest across 4 stable-hash-keyed shard tests (1.5–5.7 s each). The native
NOT-EXISTS pathology is a real `gmeow-sparql-eval` perf gap, tracked separately;
fixing it removes the query from `OFF_GATE_HEAVY` and it rejoins the gated shards.

## Patterns for keeping tests fast

- **Shard an eval-bound sweep** across N `#[test]`s keyed by a *stable* hash of a
  repo-relative identity (not positional `index % N`, which reshuffles on growth).
  nextest parallelises the shards; the shared load is paid per shard but cheap.
- **Share an expensive fixture once per run.** nextest runs each test in its own
  process, so in-process `OnceLock`/`lazy_static` does NOT share across tests — a
  cross-process cache (a serialized artifact built once) is required. The docs-model
  cluster uses this: `gmeow_docs::fixture` caches the built `DocsModel` *and* the
  rendered English site to content-addressed files, and the cache is **primed once
  before the run** (the `prime-docs-fixture` example, run by the Makefile lanes and
  the CI test job) so no test pays the build, the render, *or* the concurrent-rebuild
  contention that inflates a cold parallel build well past the budget. A genuine miss still falls through to a build,
  so a plain `cargo test` works. (nextest setup-scripts would be the natural home for
  the prime step, but they remain experimental — an explicit pre-nextest step avoids
  the opt-in flag.)
- **Carve out, don't time-out.** An irreducibly heavy test goes to `maint-heavy`,
  never onto the gate with a long per-test override.

## 2026 testing-crate choices

Adopted: `cargo-nextest` (process isolation + profiles + `default-filter`),
`datatest-stable`, `proptest`, `insta`, `criterion` — all already in tree.
`rstest` is recommended for the ~37 near-identical `crates/validate/tests/
conformance_*.rs` SHACL twin files (parameterized cases) — a tracked cleanup.
`divan` / `cargo-mutants` / `bolero` were evaluated and deferred: criterion +
`cargo fuzz` + `cargo-mutants` (already wired, report-only) cover their roles, and
switching carries baseline/scoreboard churn without serving the 25 s goal.

## Open follow-ups

1. **Native `FILTER NOT EXISTS` performance** — index the anti-join in
   `gmeow-sparql-eval` so `class-without-stereotype.rq` leaves `OFF_GATE_HEAVY`.
2. **`rstest` cleanup** of the `conformance_*.rs` SHACL twins.
