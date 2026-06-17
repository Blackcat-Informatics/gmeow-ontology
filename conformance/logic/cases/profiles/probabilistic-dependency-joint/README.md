<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# profiles/probabilistic-dependency-joint

**Acceptance criterion (#506):** marginals under a declared **dependency** model
(explicit joint), demonstrably different from the independence reading.

`a` and `b` are perfectly correlated under the declared `logic:DependencyModel`.
The explicit joint table (`:- joint(p, ...)` rows) is:

| outcome | a | b | joint probability |
|---------|---|---|-------------------|
| `joint(0.5, a, b)` | T | T | 0.5 |
| `joint(0.5)`       | F | F | 0.5 |

(The two outcomes are exhaustive and sum to `1.0`; the `(T,F)` and `(F,T)`
assignments have zero mass — `a` and `b` always agree.)

## Marginal enumeration

`both(s, on) :- a(s, on), b(s, on)` is derivable only in the outcome where both
hold:

`P(both) = Σ_{outcome : both ∈ model} jointProbability = 0.5` (the `(a∧b)` row).

**Contrast with independence:** if `a` and `b` were read as independent with
marginals `0.5` each, `P(both)` would be `0.5 · 0.5 = 0.25`. The golden value is
`0.5`, proving the declared joint — not an assumed independence — governs the
computation.

**Expected:** `?- ex:both(ex:s, X)` → `X = <…/on>` with `probability = 0.5`,
status `ok` (see `expected/answers/both.json`).
