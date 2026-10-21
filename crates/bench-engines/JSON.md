<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Benchmark JSON contract

The benchmark binary owns its free-form JSON through `purrdf_lex::json`.
Every wire boundary recursively sorts object names and keeps the last repeated
member. Array order is significant. Native builders emit integer counts,
booleans, strings and containers; compact descriptors and two-space pretty
reports preserve those bytes. Compact and pretty writer helpers add no newline.
The cost artifact adds one final newline; report `println!` calls add one.
No JSON construction or serialization moves inside an engine measurement.

Baseline admission requires an object with a `cases` array. Each case is an
object with unique nonempty `(corpus, case)` strings, a recognized `fragment`,
nonempty `world`, unsigned `golden_rows`, an object `agreement`, and an object
`native` with a nonempty `engine`, unsigned `consumed_steps` and
`peak_live_bytes`, and an array `cost_vector`. Backward cases require unsigned
`answer_count` and a nonempty `answers_fingerprint`. The other fragments require
unsigned `derived_count`; forward, incremental and incremental-grounding also
require a nonempty `rows_fingerprint`. Existential results do not carry that
fingerprint. Forward plan-cache and provenance records are not required for
other fragments. Both incremental fragments require an object `native.retraction`
with a nonempty `rows_fingerprint`. Plain incremental results also require
unsigned `signed_change_count` and nonempty `signed_changes_blake3` at both
native and retraction levels, and unsigned retraction `derived_count`.
Incremental-grounding retractions do not carry that count. These retraction and
signed-change outputs are exact comparison identities; joined rows, probes,
inner iterations and nested peak costs are not added to the improvement gate.
This shared admission applies to both sides of exact replay
and prior-baseline improvement comparison.

Exact replay retains the compact native/agreement descriptor, including exact
peak measurements, while removing the established advisory total-allocation
fields and provenance allocation-count exclusions. A separate output-identity
comparison retains fragment, world and reference identity. Prior-baseline
improvement comparison checks evaluation counts, vectors, output fingerprints,
engine/profile, reference and agreement as sorted compact tokens. Its peak
measurement remains directional. Allocation bands, the 42-count jitter floor,
and the advisory status of improvement allocation totals are unchanged. Missing
advisory totals are reported as unavailable; no aggregate total is claimed when
one is missing.

Parsed numbers retain native decimal lexemes, including exponents and signed
zero. Native numeric equality is exact decimal equality, so `1`, `1.0` and
`1e0` compare equal as numbers but serialize to different compact tokens.
Deterministic comparison uses those tokens, rather than numeric `Value`
equality. Required unsigned counters accept only unsigned integer lexemes within
`u64`; fractions, exponents, negative numbers (including `-0`) and overflow are
refused. Arbitrary native JSON numbers are not rounded through binary64.

Finding reports project existing shared divergence-ledger witnesses. Every
projected finding requires nonempty `finding_iri` and `anchor_iri`; missing
identities are actionable errors. The binary preserves those IDs and antecedent
edges, and adds the message only on the report profiles that already include it.
It does not mint replacement identities.

The pipeline's typed cost-artifact reader remains a separate consumer of this
wire contract. Pure JSON tests use authored values and temporary baseline files;
they do not run the benchmark, engine, corpus loader or producer. A JSON owner
migration supplies no new performance measurement or speedup claim.
