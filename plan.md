# Issue 1776 — native performance report JSON

Base `812aeaa34`, after PR 1919 cleanup. This delivery combines the complete
`perf-gate-merge` envelope and Criterion timing/baseline report families. They share
one qualification and integration cycle; parent issue 1776 remains broader.

## Actual caller and source contract

Makefile `perf-gate` writes validate, sync and reason-verify timing JSON before
calling this binary. `dev_validate.rs` emits `validate`; both cache-hit/miss
branches in `dev_sync.rs` emit `sync`; `dev_reason.rs` emits `reason-verify`.
All provide object-valued `deterministic_work` and `observations`. Inner profiles
vary: sync hits omit timings/stage-work and reason-verify budgets/metadata may be
null. The merger owns only the envelope and these two object projections.

`perf-evidence::sample` later reads the aggregate through native JSON, retains its
actual-byte SHA-256 and payload, extracts command inventory, and reads sync causal
counters. This delivery neither changes that consumer nor mints input identities.

## Implementation

- Native `read_document` is the sole parser, resolving names recursively last-wins.
- Bind each fixed filename to its exact command label. Require an object root and
  both projection objects; missing/mistyped fields fail through existing graded
  diagnostic machinery using dedicated `.input`. Parse/read/write failures retain
  their existing sites. No null projection fallback remains.
- Keep inner records, nulls, unknown fields, empty objects and input versions
  opaque. Native RFC numbers retain lexemes, deliberately replacing Serde's
  numeric normalization/range admission; no numeric identity conversion occurs.
- Wire the actual run path through the same pure reader/admission and merge
  helpers tested below. Preserve file order in all three arrays, full resolved
  command documents, aggregate version 1/command perf-gate, sorted pretty2+one LF.
- Leave reporters, stage timing boundaries, filesystem reads/writes and exits
  intact. The native writer is infallible for admitted values, so the obsolete
  Serde serialization-error branch is removed without a weaker alternative.
- Nine `json_perf_gate_` controls use authored JSON only, without timing, engine,
  fixture, corpus or producer activity.

## Exact source inventory

1. `crates/pipeline/src/bin/perf_gate_merge.rs`
2. `crates/pipeline/src/bin/perf_gate_merge/json.tests.rs` (new nested test module)
3. `crates/pipeline/PERF-GATE-JSON.md` (new contract)

No manifest, lock, producer, telemetry generator, sampler or sibling binary edit.
Root owns formatting, checks, integration and publication; implementer is source-only.

## Related timing family

The sequential five-file timing implementation and real caller inventory are in
implementation-bench-timing.md; its authored control inventory and scope limits
are in validation-bench-timing.md. Independent review is review-bench-timing.md.
Native typed baseline decoding, native free-form Criterion input, sorted baseline
emission and committed leaderboard use one timing implementation. Comparison is
fallible for missing/invalid required data, while valid timing regressions remain
advisory. Empty declared inventories remain distinct from absent sources; emission
requires actual selected measurements. Zero reference medians have unavailable
percentages. Other engine-cost/soak Serde owners remain unchanged.

Root formatting passed. Initial focused build exposed a redundant Object.into
type-inference ambiguity at the generic Object.insert boundary; root removed
only that conversion, preserving the actual Object and all expectations. The
original failed-build log is retained as native-tests-initial.log. Qualification
is rerunning. No corpus, measurement or producer operation is authorized locally.
