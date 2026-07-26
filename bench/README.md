<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# `bench/` — the committed perf reference run

`baseline.json` is the **committed reference run**: a flattened snapshot of a
criterion benchmark pass, one entry per `"<group>/<bench>"` with its `mean_ns`
and `median_ns` point estimates in **integer nanoseconds**.

It is the single source of truth for the committed perf leaderboard
(`generated/bench/leaderboard.md`, the `stage-export-bench` generator) and the
baseline the report-only regression scoreboard compares a live run against.

## Why integer nanoseconds

Timings are non-deterministic, so the committed artifacts must not encode raw
`f64`s that re-serialize differently across runs. The baseline rounds every
estimate to an integer ns at the emit boundary, so both `baseline.json` and the
rendered `leaderboard.md` are formatting-stable and survive the strict `sync`
drift gate. The drift gate only *reads* this file — it never runs benchmarks.

## Refreshing the baseline (maintainer only)

There is **one** producer — the Rust `bench-compare --emit-baseline` path:

```sh
make maint-bench-baseline   # runs `make bench`, then emits bench/baseline.json
git add bench/baseline.json generated/bench/leaderboard.md
```

Refreshing is a deliberate, hand-committed act — never auto-drift. Numbers are
machine-specific evidence; record exactly what ran and do not hand-edit them.

## Report-only regression scoreboard

The off-gate `suite-quality` CI lane runs `make bench-compare`, which prints a
`live run vs baseline` scoreboard (`ok | watch | regressed`) to the job summary.
It is advisory: runner jitter is expected and it **never** fails a PR
(Principle 18 — the authoritative gate stays native-first and Docker-free).

# `cost-baseline.json` — the committed deterministic engine-cost reference run

`cost-baseline.json` is the **committed deterministic cost/agreement baseline**:
the `gmeow-bench-engines --emit-cost` artifact over the committed mini corpora
(`conformance/logic/cases/bench/`). Unlike `baseline.json` (criterion timings),
every value here is an **integer count or a fingerprint or a boolean verdict** —
per `(corpus, case, engine)` the sorted cost-vector tuples, `consumed_steps`, the
derived / answer counts, the deterministic `peak_live_bytes` allocation scalar,
the verdict-agreement tokens, and the per-corpus divergence-ledger tally. It
carries **NO** wall-clock, **NO** peak-RSS, and **NO** total-allocation scalars
(those are report-only in the harness), so the bytes are a pure function of
`(engine version, corpus)` — byte-identical across runs.

It is the single source of truth for the committed cost ledger
(`generated/bench/cost-ledger.md`, the `stage-export-cost-ledger` generator), a
drift-gated projection reproduced byte-for-byte from this file without ever
running a benchmark. The strict `sync` gate only *reads* this file.

## Refreshing the cost baseline (maintainer only)

There is **one** producer — the Rust `gmeow-bench-engines --emit-cost` path:

```sh
make maint-bench-cost-baseline   # emits bench/cost-baseline.json (offline; twice-diffed for byte-stability)
make regen                  # re-projects generated/bench/cost-ledger.md
git add bench/cost-baseline.json generated/bench/cost-ledger.md
```

Refreshing is a deliberate, hand-committed act — never auto-drift. The counts are
attributable to the pinned engine revisions recorded in the artifact's
`engine_pins`; do not hand-edit them.

## Cost-regression finding (richer honesty surface)

`gmeow-bench-engines --check-cost bench/cost-baseline.json` compares a FRESH cost
run against the committed baseline and, on **any** deterministic-count divergence
(a changed count/fingerprint/verdict, a dropped case, or a case absent from the
baseline), emits a `reason.divergence.corpus-only` `gmeow:Finding` routed through
the shared divergence ledger (`divergence_diag_ledger` — content-addressed
`finding_iri` + anchor + antecedents) and hard-fails. The primary on-gate gate is
the strict `sync` drift check on `cost-ledger.md`; this mode is the richer finding
surface behind it.
