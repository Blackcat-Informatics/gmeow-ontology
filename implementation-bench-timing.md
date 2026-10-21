# Criterion timing JSON family

Sequential extension after the perf merger source freeze; preserve all three
perf-merger files and existing Stage evidence. Own only the timing portion of
stages/bench.rs, its actual bench-compare CLI, timing controls and wire docs.
CostArtifact/engine-cost/soak Serde remains untouched.

Use native Record/from_slice for typed baseline values (known duplicates refuse,
outer map last-wins), read_document for free-form Criterion values, and native
Object/pretty writer for sorted integer-ns output plus one LF. Preserve valid
measurement rounding, positive-denominator percentages and >5/>15 bands.

Report-only means valid timing regressions remain advisory. Invalid/missing
required baseline/live inputs must return a graded failure, not fabricated
all-new/all-missing reports. Valid {} baseline remains explicit empty inventory;
an existing empty current directory may report missing. Empty/missing live
measurement emission refuses to publish a baseline. A zero baseline denominator
has unavailable delta/status. Refuse malformed, negative, nonfinite and
out-of-u64 measurement values with file/benchmark/metric context.

Tests use only authored JSON/plain records and tiny temporary trees, never real
benchmark/criterion/engine/corpus/producer execution. Root owns formatting,
qualification, lock resolution and publication. No execution by this lane.
