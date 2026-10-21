The benchmark binary now uses the native JSON owner for artifacts, reports and baseline readers. Sorted object tokens, integer bytes, array order and newline policies remain explicit. Both comparison paths reject malformed or duplicate cases and compare actual backward and incremental/retraction output identities, so reduced memory cannot conceal changed answers. Reports require existing ledger finding and anchor identities; full-width counters produce safe signed display deltas.

Measurement callbacks, allocator/Rayon setup, allocation bands, jitter floor and peak direction remain unchanged. This supplies no new performance measurement.

Validation:32 producer-independent authored JSON/ledger controls passed, including all16 existing controls; strict producer-independent prebuild Clippy, helper hygiene, formatting, source recipe comparison and independent review passed. Actual benchmark/baseline replay, corpus, wasm/installed and full acceptance remain unrun under the recorded campaign exception. Issue1776 remains open.

Refs #1776. Principles4,7,17.
