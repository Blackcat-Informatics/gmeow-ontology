<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-conformance

`gmeow-conformance` supplies native logic-case execution and golden comparison.
The optimized pipeline producer runs the authored cases, sharing its native rule
library and caching each case independently. Tests consume those authenticated
observations and compare them against the committed goldens; they never compile
or materialize the corpus.

## Source Map

| Module | Responsibility |
| --- | --- |
| `discover` / `paths` | Locate case directories and repo-relative assets. |
| `profile` | Parse `profile.json` and enforce required case metadata, including the `shipped_rules` IRI list. |
| `run` | Execute the native compiler, certifier, materializer, query, and explanation cores; resolve `shipped_rules` against an explicitly supplied compiled library. |
| `observations` | Produce typed DL/numeric oracle results and shared TPTP decision/proof observations. |
| `consistency` | Observe a parsed native dataset once, preserving full verdicts and world counts for shared producer actions. |
| `serialize` | Write N-Quads and canonical JSON artifacts for comparison/reporting. |
| `compare` | Diff RDF by graph isomorphism, JSON canonically, and explanations by cited-IRI skeleton. |
| `external` / `divergence` | Ingest external corpora and emit divergence findings. |

## Checks

Produce fixtures explicitly before selecting conformance tests. `make conformance`
authenticates existing results and fails on a missing or stale producer selection.
`make conformance-report` is a separate maintenance producer, not a test setup step.
The required consistency, EL/entailment divergence, full-decided and fragment-family
checks read the pipeline producer's shared observations. Execution failure never
stands in for an honest logic gap. The independent DL/numeric oracle and authored
TPTP proof checks consume the same authenticated producer boundary. Their original
semantic assertions, proof identities and terminal-export goldens remain required.

The exhaustive full-divergence suite requires a separately produced selector:
after the required fixtures, run `make produce-conformance-heavy-test-fixtures`,
then `make maint-rust-heavy`. Both soundness and frozen-gap assertions consume that
same observation; the test runner has no producer fallback.

```bash
make conformance
make conformance-report
make rust-docs
```

Diagnostic meta-rule and gate-morphism checks also consume authenticated producer
observations. The producer borrows the shared grounding compilation and the native
diagnostics category wiring, records every current grade and each negative fixture,
and retains antecedent traces, cluster edges and report-enrichment evidence.
Tests compare those observations against the independent Rust gate policy and
expected findings without compiling or executing authored modules.

The OntoUML divergence consumer reads producer-recorded foundation discipline
sets. Its authored model is lowered directly into a native dataset; serialized
N-Quads are emitted only when an external corpus export requests them. Invalid
source or native execution never satisfies an expected unsupported construct.

RDF codec syntax, canonical ordering, serialization round trips and dataset
deduplication are tested by PurRDF. OntoUML lowering checks here inspect typed
GMEOW facts and selected reasoning worlds; they do not recheck those substrate
contracts. A single observation-level check preserves GMEOW's distinction
between malformed input and an honest logic capability gap.
