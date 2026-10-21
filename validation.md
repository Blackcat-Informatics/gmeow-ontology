# Native performance report JSON acceptance index

| Behavior / check | Command and actual result | Scope / limits |
| --- | --- | --- |
| Merger envelopes, full documents, native numeric tokens, profiles and refusals | make nextest-synthetic with package(gmeow-pipeline) and test(json_perf_gate_) included in combined selection:9 PASS | Authored bytes through actual helpers; no timing input execution |
| Timing baseline/live admission, exact output, reports, paths and unavailable percentages | Same combined selection:13 json_bench_timing_ plus5 named existing timing controls PASS | Authored JSON/plain values/tiny temporary trees; no benchmark or corpus |
| Combined focused execution |27 executed,27 PASS,1083 outside selection skipped; native-tests.log | Four embedded-consumer crates excluded by the supported synthetic target; no full suite claim |
| Initial compilation | native-tests-initial.log: E0283 redundant Object.into; removed only redundant conversion | No expectations weakened; complete rerun above passed |
| Strict prebuild Clippy | make clippy-prebuild: exit0,26.42s | Workspace/all targets -D warnings on producer-independent graph; same four consumers excluded |
| Helper ownership | make helpers-hygiene: exit0, helpers.log | Full existing source helper policy, no debt/config changes |
| Producer source admission | make producer-recipe: exit0, producer-recipe.log | Source identity resolution only; no optimized build or corpus production |
| Formatting and whitespace | make fmt exit0; git diff --check exit0 | Root-owned formatting |
| Independent reviews | review-perf-merge.md and review-bench-timing.md clear | Real callers, codecs, failure paths, wire contracts and authored controls reviewed |

Exact focused filter: package(gmeow-pipeline) and (test(json_perf_gate_) or test(json_bench_timing_) or test(emit_baseline_is_sorted_integer_ns) or test(compare_classifies_ok_watch_regressed_missing_new) or test(compare_with_explicit_empty_baseline_marks_all_new) or test(compare_refuses_malformed_baseline) or test(compare_refuses_missing_criterion_root)). Test-corpus scanner/purity prerequisites passed. Native-tests.log preserves actual runner evidence, not compilation alone.

Native perf-gate free-form numbers deliberately retain RFC lexemes including large/fraction/exponent/signed-zero tokens; no inner identity is reinterpreted. Baseline timing fields instead require unsigned integer tokens through u64max, reject coercion and known duplicates. Live timing floats retain valid rounding but refuse invalid/negative/overflow data. Missing required inputs fail, valid empty declared inventories remain explicit, baseline emission requires measurements, and valid regressions remain advisory. Both wire documents describe these owner rules. Other cost/soak Serde remains untouched.

User retained the campaign exception: no local optimized producer, corpus/fixture construction, sync or full gate; sequential merges without waiting for CI. Actual perf-gate telemetry, Criterion measurements, produced/installed/corpus parity, full consumer-inclusive Clippy and issue1776 acceptance remain unrun. No performance gain claimed. Normal signing/commit/push hooks must run fully; their actual results are retained separately before merge. No manifest, lock, fixture, authentication, threshold, build-profile or generated-source edit.

Normal commit and push both exited0 with required hooks; commit.log and push.log retain their actual results. PR1920 was created once despite stagectl returning its known JSON-parser error; forge lookup confirmed the existing PR and creation was not retried.
