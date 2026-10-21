<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Cost baseline reader contract

The engine-cost ledger and soak report decode the same
`gmeow.bench-engines.cost/1` artifact through native
`purrdf_lex::json::record::FromJson` and a shared admission boundary.
The native benchmark executable remains the artifact's writer. These consumers
read stored measurements; they never execute engines or refresh a baseline.

The required envelope contains `schema`, `case_count`, `engine_pins`,
`cases`, `rule_parallelism` and `ledgers`. The schema must name the supported
version, case_count must equal the case-array length, cases and ledgers must be
nonempty, and case identities must have nonempty corpus/case names with no
duplicate pair. Ledger tally case counts represent comparisons and are not
required to equal the raw case-record count.

Both engine pins, each ledger corpus key and each stored finding-graph identity
must be nonempty. Every ledger requires at least one comparison, and ledger
corpus keys must exactly match the case corpus inventory. These are the actual
producer's scope and nonvacuity guarantees; comparison multiplicities vary by
fragment. The reader does not recompute stored graph identities or impose a new
digest format.

Records require objects, required fields and their declared types. Unknown
record fields are ignored; known duplicates are refused. This replaces the
former Serde traits, including incidental positional-array record admission.
The ledger map retains sorted keys and duplicate outer keys remain last-wins.
Nullable optionals admit absent or null values without inventing measurements.

Unsigned counts use exact integer tokens through `u64::MAX`, with checked
`u32` bounds for strata and proof heights. They reject signed zero, negative
values, fractional/exponent tokens and overflow. Signed provenance overheads
use exact `i128` tokens, including both extrema; signed `-0` remains zero.
Fractions, exponents and overflow are refused for signed overheads too. No
binary floating point participates in this reader or its output.

Each cost-vector item is exactly
`[rule_string, predicate_string, stratum_u32, derivations_u64]`. Array order is
preserved. Malformed items produce native field/index JSON pointers.

Backward records carry exactly answer_count; forward, existential, incremental
and incremental-grounding records carry exactly derived_count. Conflicting,
missing or fragment-inappropriate count evidence is refused. Selected
incremental profiles require their scratch comparator, joined_rows and both
insert/retract parity observations. Incremental-grounding also requires its
grounding record and scratch probe/rule counts. These selected obligations also
apply when the corresponding optional report panel is present. Forward plan
cache and provenance panels remain optional; no other fragment is forced to
provide them.

The existing nonvacuous four-worker evidence validation remains unchanged.
Soak output still refuses any nonzero corpus_only or dl_gap tally, and its
combined digest still hashes lexically sorted ledger keys with the exact
`corpus + US + finding_graph_blake3 + LF` framing.

Reports retain their case/key order, integer values, arithmetic and final
newline. A legitimately absent max_proof_height now renders an em dash,
distinct from a measured zero. Missing selected join/parity evidence now fails
instead of being displayed as zero/false. No baseline, measurement tolerance,
profile, soak window or benchmark execution behavior is changed.
