# Validation

Source handoff is frozen. The source implementer ran no checks. Root qualification
is recorded below; harness, benchmark, corpus and installed acceptance remain unrun.

Files: crates/bench-engines/Cargo.toml, src/main.rs, src/json_tests.rs, JSON.md.
No Cargo.lock, pipeline consumer, source-policy capability, measurement, engine,
profile, allocation band or jitter-floor edits. The source input consists of
ordinary native Object/ToJson builders; no new macro capability is requested.

Focused new controls: package(gmeow-bench-engines) and test(json_bench_).
The entire package's unit tests are authored JSON/temporary files/shared diagnostic
ledger controls, plus the retained invalid-window soak guard before engine entry.
There are sixteen new json_bench_ controls, and all sixteen existing pure tests
remain in the external unit module. No Cargo integration target is added.

Covered in source: literal sorted compact descriptor and pretty Unicode report
bytes/newlines; nested last-wins parsing and malformed/trailing input refusal;
u64 maximum/overflow/fraction/exponent/negative/signed-zero admission; exact decimal
equality versus distinct compact tokens; backward and incremental result drift
despite lower peak; mandatory fields missing/mistyped on both sides; duplicate and
empty case identities; all five minimum profile distinctions; advisory unavailable
reporting; full-u64 signed delta safety; preservation/refusal of actual shared
ledger IDs. Existing peak-drop/nonregression, advisory balloon, allocation band,
provenance exclusions, rule-parallel and dropped-case expectations remain.

Production entry points remain Make bench-golden-gate, bench-soak,
maint-bench-engines and maint-bench-cost-baseline. The typed pipeline CostArtifact
reader is unchanged. Actual harness/committed baseline and hosted gate acceptance
are unrun; pure source fixtures do not substitute for that evidence. No performance
improvement is claimed. Memory recall timed out; current owner source confirms
native read_document/sorted_last_wins, strict u64 selectors, exact decimal numeric
equality and preserved number display lexemes.

## Root qualification

| Check | Result and scope |
| --- | --- |
| make fmt; git diff --check | Both exited0; no unrelated source formatting |
| make nextest-synthetic NEXTEST_FILTER='package(gmeow-bench-engines)' | Exited0;32 executed,32 passed,0 skipped.16new+16retained authored JSON/ledger controls. No benchmark engine or corpus case executed. Exact output native-tests.log |
| make clippy-prebuild | Exited0; strict workspace/all-target -D warnings on producer-independent inventory,6.86s. Four consumer-inclusive packages remain excluded by the repository target; not a full Clippy verdict |
| make helpers-hygiene | Exited0; ownership/dependency/hash-domain controls, including source tests. Exact output helpers-hygiene.log |
| make producer-recipe on unchanged main and this same-base checkout | Both exited0; exact stdout files compare equal. Only source identity inspected; no optimized producer built or corpus produced |
| Independent review | review-independent.md: no blocking source finding; actual builders inspected, not executed |
| Normal commit/push hooks | Both exited0, normal signing/hooks; exact output commit.log and push.log. No bypass |

Cargo resolved only the benchmark package dependency edge from serde_json to
purrdf-lex; no pin or other package graph changed. Final source inventory is five
files including Cargo.lock, and two new source/doc files were explicitly inspected
despite local status.showUntrackedFiles=no. No source-policy exception or local JSON
macro shim was introduced. User exception remains no local full gate/corpus and
sequential ghprsq delivery ignoring CI; actual acceptance stays visible above.
