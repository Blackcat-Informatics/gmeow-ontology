<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# profiles/probabilistic-confidence-guard

**Acceptance criterion (#506):** a `logic:confidence` annotation is **not**
auto-interpreted as a probability — a guard test that goes **red if violated**.

`diagnosis(patient, flu)` is an asserted fact (in `input.nq`) that also carries a
`logic:confidence` of `0.9` (the `:- confidence(...)` directive). Under
`logic:ProbabilisticProfile` with a declared `logic:FullIndependence` model:

- The fact is deterministic (asserted), so its marginal is **`1.0`**.
- The confidence value `0.9` is metadata about the asserter's sureness — it is
  **never** read as a probability (LOGIC-SEMANTICS §Confidence, probability,
  weight, and evidence: *"A `logic:confidence` annotation MUST NOT be interpreted
  as a probability unless an explicit mapping to `logic:probability` is
  declared."*).

**Why this is red-if-violated:** if an implementation promoted `confidence` into
the probability model, the binding would carry `probability = 0.9`. The golden is
`1.0`, so any such leak fails this case (and the dedicated guard assertion in
`tests/test_logic_probabilistic.py` checks `probability != 0.9`).

**Expected:** `?- ex:diagnosis(ex:patient, X)` → `X = <…/flu>` with
`probability = 1.0`, status `ok` (see `expected/answers/diagnosis.json`).
