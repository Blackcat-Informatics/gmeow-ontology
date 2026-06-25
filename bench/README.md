<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# `bench/` — the committed perf reference run (#668)

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
rendered `leaderboard.md` are formatting-stable and survive the `check-generated`
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
