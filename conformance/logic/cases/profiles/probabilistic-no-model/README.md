<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# profiles/probabilistic-no-model

**Acceptance criterion (#506):** probabilistic inference **requires** a declared
independence/dependency model — without one it refuses.

A probabilistic fact (`rain = 0.5`) is declared, but the program declares **no**
`:- probability_model(...)`. The evaluator must **refuse**: it returns status
`unknown` with no bindings rather than silently assuming `logic:FullIndependence`
over the `logic:probability` facts.

This is the same epistemic-hygiene contract as the confidence guard, on the other
axis: just as `logic:confidence` is never *promoted* to a probability, a
`logic:probability` fact is never *interpreted* without an explicitly declared
model. Defaulting to independence would be the named failure mode.

**Expected:** `?- ex:wet(ex:today, X)` → no bindings, status `unknown` (see
`expected/answers/wet.json`). A computed marginal here fails the case.
