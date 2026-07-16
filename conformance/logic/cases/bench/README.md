<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Engine-benchmark corpus

This tree is the **performance** vendored family — the engine-vs-engine sibling of
the correctness suites in [`../external/`](../external/README.md). Both families
share the one vendored-corpus contract documented in
[`../README.md`](../README.md): the same `corpus.json` schema and the same
`audit_vendorable` license gate (a non-vendorable license is a HARD FAIL, never a
silently-loaded case).

Unlike the `external/` correctness corpora — graded against a third party's
*published verdict* — a bench case grades the engine against a **hand-derived,
known-correct row/answer count**. The loader lives at
`crates/conformance/src/bench_corpus.rs`; the driver is the `gmeow-bench-engines`
binary. These corpora are consumed by the benchmark harness, NOT by
`stage-conformance`, so they never enter the agreement matrix.

## Layout

```text
cases/bench/<corpus>/
  corpus.json                 # the shared vendored contract (schema + SPDX license)
  <case>/
    profile.json              # { "fragment": …, "engines": [ … ] }
    program.rules             # corpus-local fixture text (forward/existential/backward)
    input.nq                  # the world-scoped EDB as N-Quads
    delta.nq                  # (incremental fragments only) one insert batch
    expected/result.json      # HAND-DERIVED golden: world IRI → { rows, digest? }
```

`expected/result.json`'s `rows` is the mathematically-correct count of derived
facts (forward/existential) or goal answers (backward), authored by formula or by
hand — never an engine echo. Loading is manual and hard-fail; ordering is
deterministic (corpora then cases, both sorted by directory name).

## Fragments

Each case's `profile.json` `fragment` selects both the rule surface and the
engines the harness may drive:

| fragment | grades |
|----------|--------|
| `forward` | forward Datalog materialization |
| `existential` | value-inventing native TGD chase |
| `backward` | goal-directed backward query (native / captured SLD golden) |
| `incremental` | signed insert/retract maintenance vs. clean native rebuilds |
| `incremental-grounding` | signed maintenance of the ground WFS/stable-model slice |

## Committed corpora

| corpus | source | refresh |
|--------|--------|---------|
| `chasebench-mini` | ChaseBench-style forward/existential fixtures | `make maint-chasebench-corpus` |
| `relational-core-mini` | OpenRuleBench-style relational fixtures | `make maint-openrulebench-corpus` |

Both are self-authored, license-clean fixtures (`CC-BY-4.0`), and
`bench_corpus::load_bench_corpora_from(root)` is the `--corpus-dir` seam a later
fetch lane can point at a full fetched-distribution corpus root.
