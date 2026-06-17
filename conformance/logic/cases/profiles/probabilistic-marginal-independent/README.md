<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# profiles/probabilistic-marginal-independent

**Acceptance criterion (#506):** marginals computed correctly under a declared
independence model.

`rain` and `sprinkler` are independent Bernoulli probabilistic facts, each with
probability `0.5`, declared under `logic:FullIndependence` (the
`:- probability_model(full_independence).` directive). The noisy-OR rule makes
`wet(today, yes)` derivable whenever either holds:

```prolog
ex:wet(D, ex:yes) :- ex:rain(D, ex:yes).
ex:wet(D, ex:yes) :- ex:sprinkler(D, ex:yes).
```

## Full total-choice (θ) enumeration

The golden marginal is the exact weighted-model-counting sum over all `2² = 4`
total choices — **not** a closed-form shortcut. Let `r = rain`, `s = sprinkler`:

| θ | rain | sprinkler | P(θ) = ∏ | `wet` derivable? |
|---|------|-----------|----------|------------------|
| 1 | F    | F         | 0.5·0.5 = 0.25 | no  |
| 2 | T    | F         | 0.5·0.5 = 0.25 | yes |
| 3 | F    | T         | 0.5·0.5 = 0.25 | yes |
| 4 | T    | T         | 0.5·0.5 = 0.25 | yes |

`P(wet) = Σ_{θ : wet ∈ model(θ)} P(θ) = 0.25 + 0.25 + 0.25 = 0.75`.

(Equivalently `1 - P(¬r)·P(¬s) = 1 - 0.5·0.5 = 0.75`, the noisy-OR identity — but
the table above is what the evaluator actually computes.)

**Expected:** `?- ex:wet(ex:today, X)` → `X = <…/yes>` with `probability = 0.75`,
status `ok` (see `expected/answers/wet.json`).
