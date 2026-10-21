<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Criterion timing JSON

The timing family uses native `purrdf_lex::json` for live Criterion inputs,
the committed timing baseline and its writer. It does not change deterministic
engine-cost or soak records.

Live inputs are selected only from paths with at least
`<group>/<benchmark>/new/estimates.json` below the supplied Criterion root.
The relative group/benchmark path is the benchmark identity; selected components
must be UTF-8. Old `base` snapshots and shallower paths are ignored.
Each selected document requires numeric `mean.point_estimate` and
`median.point_estimate`. Free-form object members retain recursive last-wins
semantics, including ignored metadata.

Measurements must be finite and nonnegative, and their rounded binary64 value
must fit unsigned 64-bit nanoseconds. Exact native decimal sign checking refuses
negative values even if binary64 would underflow to negative zero. Actual zero
lexemes remain valid. Valid values retain binary64 conversion and rounding to
nearest integer nanoseconds, with halfway cases away from zero. Values that
round to or beyond 2^64 are refused rather than saturated.

The baseline maps benchmark names to object records with required
`mean_ns` and `median_ns` unsigned integer tokens. Fractions, exponents,
negative tokens including `-0`, nulls and overflow are refused. Unknown record
fields are ignored; known duplicates are refused. Outer map duplicate names
remain last-wins. Native typed records require objects rather than incidental
positional arrays. Exact integers through `u64::MAX` are retained.

Baseline emission orders benchmark names lexically, emits `mean_ns` before
`median_ns`, uses two-space pretty JSON and appends exactly one newline.
It refuses a missing root or an empty selected measurement inventory.

The comparison requires a readable, valid baseline and an existing live input
directory. Invalid required inputs now fail with graded diagnostics; they no
longer manufacture an all-new or all-missing report. A valid `{}` baseline
explicitly declares an empty reference inventory and makes live entries new.
An existing empty live directory explicitly makes reference entries missing.

Valid comparisons remain advisory, including regressions. Positive-denominator
percentages retain the existing calculation: watch strictly above 5%, regressed
strictly above 15%. A zero reference median has no defined percentage; its delta
is an em dash and its status is `unavailable`, without incrementing either
regression tally. Missing entries remain distinct from measured zero.
The committed leaderboard remains a pure projection of the required baseline,
with no live measurements or benchmark execution.
