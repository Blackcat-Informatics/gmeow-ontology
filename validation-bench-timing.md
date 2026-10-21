# Timing source handoff

Source-only extension. No formatter, build, checks, tests, host/resource probe,
producer/corpus/model/benchmark/Criterion execution, lock mutation or publication
was performed by this lane. Original perf-merger Stage evidence is unchanged.

## Exact five-file source inventory

1. crates/pipeline/src/stages/bench.rs — timing-only native codecs, collection,
   encoding, comparison and committed timing leaderboard reader.
2. crates/pipeline/src/bin/bench-compare.rs — actual fallible comparison and
   graded missing/unreadable baseline handling.
3. crates/pipeline/src/stages/bench.tests.rs — five existing timing controls
   adapted to fallibility, explicit {} baseline and required-input refusal.
4. crates/pipeline/src/stages/bench.timing_json.tests.rs —13new pure controls,
   including one Unix-specific authored invalid-path test.
5. crates/pipeline/BENCH-TIMING-JSON.md — wire/admission/output/error contract.

No perf-merger files, CostArtifact/engine-cost/soak Serde, baseline files,
manifests, locks, thresholds, profiles or source selection were changed.

## Qualification for root

New filter: package(gmeow-pipeline) and test(json_bench_timing_)

13new controls independently pin complete sorted JSON+LF and valid report/
leaderboard Markdown; exact u64max and strict unsigned admission; known
duplicates versus outer-map/freeform last-wins; live float rounding/exponents;
negative-underflow/nonfinite/out-of-range/missing/wrong metric refusals with
file/benchmark/metric context; malformed/trailing/nonUTF8 documents; threshold
boundaries; missing/new/zero-denominator distinction; empty versus missing input;
selected new/base/shallow paths; selected nonUTF8 benchmark identity refusal.
Only authored JSON/plain values and tiny temporary files are used.

Retain the five existing timing tests: emit_baseline_is_sorted_integer_ns,
compare_classifies_ok_watch_regressed_missing_new,
compare_with_explicit_empty_baseline_marks_all_new,
compare_refuses_malformed_baseline, compare_refuses_missing_criterion_root.
The two unrelated cost-ledger tests are preserved, not needed for this focused
timing selection. Other whole-pipeline and actual Make/CLI/hosted measurements
remain unrun. Pipeline still needs its normal Serde dependencies for other owners.

Pinned native source: record.required_with attaches native member pointers;
Number.as_u64 refuses signed zero and coerced tokens; Number.decimal().is_negative
detects negative underflow while treating exact zero as nonnegative. Original
native parse and I/O errors remain live through Diag::from plus context.
Source is frozen for root qualification; no runtime success claimed.
