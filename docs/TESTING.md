<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Testing

GMEOW's doctrine is "one canonical source, everything else a generated or
checked projection". Tests follow the same rule: a slice's behavioural tests
live in its `tests/` directory **as ontology data** in the test-DSL vocabulary
(`dsl/tests/vocabulary.ttl`), and a native Rust harness executes them. This is
faster than per-test Python (oxigraph SPARQL, no interpreter per assertion) and
keeps tests inspectable and projectable like every other GMEOW term.

This document describes the test-DSL, the native harness, and — in detail — the
**competency-question reasoning model**, which is the one place the harness makes
a deliberate cost/fidelity trade-off.

## The two test layers

| Layer | Lives in | Runs under | Covers |
|---|---|---|---|
| Declarative slice tests | `slices/**/tests/*.ttl` | `crates/slicetest` (cargo-nextest) | structural invariants, competency questions, example conformance |
| Bespoke Python tests | `tests/`, `slices/**/tests/test_*.py` | pytest | assertions the declarative form can't express (numeric/temporal logic, generated-artifact checks, dynamic-set sweeps) |

The repetitive rdflib structural/competency tests now live entirely in the
declarative layer, executed by the native Rust harness. Nothing is silently
dropped: every structural invariant and competency question is expressed as a
declarative cell that executes on every gate.

## The declarative test-DSL

A `tests/*.ttl` spec file holds instances of three cell types. The harness
discovers files by fixed name (`datatest-stable` glob, one nextest case per
file) and runs every cell in the file, aggregating per-cell failures.

### `gmeow:CompetencyQuestion` — `tests/competency.ttl`

A SPARQL ASK/SELECT plus its expected outcome. The query is inline
(`gmeow:cqQuery`) or referenced (`gmeow:cqQueryFile`, **repo-root-relative**).
An ASK pins `gmeow:cqExpectAsk`; a SELECT pins its rows by full enumeration
(`gmeow:cqExpectRow` + `gmeow:cqExactRows true`, the preferred maximal-fidelity
form), by an enumerated **subset** (`gmeow:cqExpectRow` with `cqExactRows`
omitted — a contains-check, for questions whose source asserted only that certain
rows are present), or, as an escape hatch, a coarse `gmeow:cqExpectRowCount`
(`0` pins an expected-empty result). Runs over the full **merged ontology** (see
*Reasoning* below).

A question that classifies **instance** data (e.g. "is this event a lie?") can
overlay a slice-relative ABox fixture via `gmeow:cqDataFile`: the fixture is
inserted onto the asserted merged graph for that one cell, the query runs, and the
fixture is removed again (the same scoped-overlay idiom `gmeow:exampleFile` uses).
Because the merged ontology is TBox-only, this is how instance-classifier
competency questions get something to match against. The overlay is honoured only
in the `gmeow:reasoningNone` lane — pairing `gmeow:cqDataFile` with
`gmeow:reasoningRdfs` is rejected (the RDFS closure is computed before the overlay,
so the fixture's entailments would be invisible).

### `gmeow:StructuralAssertion` — `tests/structural.ttl`

A MUST / MUST-NOT (`gmeow:saPolarity`) triple pattern (`gmeow:saPattern`, a
SPARQL ASK) or SHACL shape (`gmeow:saShape`) over a slice's module graph, or the
module plus its `examples/` (`gmeow:saScope`, default `scopeModuleAndExamples`).
No reasoning — these constrain the asserted shape of the graph.

### `gmeow:ExampleConformance` — `tests/example-conformance.ttl`

Binds an example file (`gmeow:exampleFile`, **slice-relative**) to its expected
validation outcome (`gmeow:expectedOutcome` conforms/violates, with
`gmeow:expectedViolationCode` in the real `shacl.<ConstraintComponentLocalName>`
form). The harness validates the example against the slice module + shapes via
the native SHACL engine (`gmeow_validate`) and compares finding codes. This is
**slice-scoped** — an example that references cross-slice classes is validated in
full by `make validate`, not here.

## Competency-question reasoning (the D+C model)

Competency questions ask what the ontology can answer ("what kinds of agent does
GMEOW model?"). The honest version of that question is what GMEOW **entails**,
not merely what is written down (CONSTITUTION Principle 7/8). But full
materialized reasoning is expensive, so the harness offers a tiered opt-in rather
than paying the maximum cost for every question.

### The lanes

`gmeow:cqReasoning` selects the entailment lane (a `gmeow:ReasoningProfile`);
omitting it means `gmeow:reasoningNone`.

- **`gmeow:reasoningNone` (default) — the asserted merged graph.**
  No materialization. SPARQL property paths (`rdfs:subClassOf*`,
  `rdfs:subPropertyOf*`) still compute transitive closure *at query time*, so a
  "what kinds of X?" question written with a path operator is answered correctly
  with a sub-second graph build. Both of the epistemics exemplars use this lane:
  `agents.rq` uses `rdfs:subClassOf*`, and the 48 contribution roles are directly
  typed, so neither needs materialized entailment.

- **`gmeow:reasoningRdfs` — the merged graph closed under RDFS.**
  Opt in when the answer depends on entailment a property path can't express:
  domain/range typing (`rdfs2`/`rdfs3`), `rdf:type` propagation up the class
  hierarchy (`rdfs9`), and subclass/subproperty/property propagation
  (`rdfs5`/`rdfs7`/`rdfs11`). The closure is computed **natively in oxigraph** as
  SPARQL `CONSTRUCT` rules iterated to a fixpoint (`crates/slicetest/src/stores.rs`)
  — seconds, not minutes. The harness builds it once per spec file, and only if
  some question in that file requests it.

### Why not full OWL 2 RL

The native OWL 2 RL closure (`gmeow_logic::reason::rl_closure`) takes minutes over
the *whole* merged ontology — far too slow for a routine slice-test lane. RDFS
captures the type-and-subsumption entailments competency questions actually need
at a tiny fraction of the cost, and ordinary `crates/slicetest` questions avoid
the full `gmeow-logic` dependency as a result.

### The OWL 2 RL entailment harness (`crates/logic/tests/ontology_entailments.rs`)

Genuine OWL 2 RL entailment tests — property chains, `owl:equivalentClass`
classification, sub-class/sub-property subsumptions, EL consistency — live in a
separate native Rust harness, **not** the slice-test DSL. `scoped_closure(slices,
abox)` parses just the relevant slice `module.ttl` files, injects a tiny test
A-Box, and runs `gmeow_logic::reason::rl_closure` over that small input. Scoping is
load-bearing: the chase is superlinear in fact count, so a one-/few-module closure
runs in ~1–90 s where the full-ontology chase takes minutes. This is the native
twin of the old `gmeow_tools` `_materialize(module, *abox)` pytest pattern (the
reasoning cluster the ~45-min `python` lane was dominated by, now migrated to native Rust).
The harness runs under the `engine` nextest test-group (memory-capped, serialized)
via `cargo nextest run -p gmeow-logic`.

### Why the default is safe

Reasoning is **monotonic** — the reasoned graph is a superset of the asserted
graph. So for the positive enumeration/ASK questions competency tests use, the
asserted default can only ever *under*-answer, never over-answer. An under-answer
against an enumerated, `cqExactRows`, or `cqExpectRowCount` expectation is a
set/count mismatch — a **loud test failure**, never a silent wrong-green. A
question that genuinely needs RDFS entailment fails until it sets
`gmeow:cqReasoning gmeow:reasoningRdfs`; a question that doesn't need it returns
the same answer either way. If a future question needs entailment beyond RDFS
(property chains, `someValuesFrom`, `sameAs`), that is a deliberate extension of
the `gmeow:ReasoningProfile` axis, not an ad-hoc value.

## Running the tests

```sh
make slicetest      # the native slice-test harness in isolation
make rust-test      # the whole Rust workspace (includes the harness)
make test           # the pytest suite
make check          # the full local gate (lint, clippy, rust-test, validate,
                    # reasoning, pytest, compliance report) — Docker/Java-free
```

The harness automatically discovers the three fixed spec filenames —
`tests/competency.ttl`, `tests/structural.ttl`, and `tests/example-conformance.ttl`
— under any slice, so a new slice's declarative specs light up with no harness
changes. Other filenames under `tests/` (e.g. `tests/counter-examples/*.ttl`,
referenced only via `gmeow:exampleFile`) are deliberately not auto-executed.
