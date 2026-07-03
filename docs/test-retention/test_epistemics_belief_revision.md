# Retention: `tests/test_epistemics_belief_revision.py`

**Category:** Domain invariant → slicetest cells (partial; fixture-data competency tests remain)

## What it tests

Competency test for the doxastic belief-revision pattern (#560).

## What has migrated

`test_ontology_constraints_and_functionality` moved to declarative
`gmeow:StructuralAssertion` cells in
`slices/core/epistemics/tests/structural.ttl` (#1120, Task 9):

- `ex:saDoxasticStateAndTenureConstraints`
- `ex:saCredenceNotFunctional`

## Why the rest cannot move to Rust today

The remaining functions assert over the fixture data in
`slices/core/epistemics/tests/fixtures/coverage/epistemics-belief-revision.ttl`
(instance existence, literal values, intervals, linked claims). These are
ABox competency checks over a worked example; they can later become
`gmeow:CompetencyQuestion` cells with `cqDataFile`, but they currently remain in
pytest as the slice's example-conformance / competency migration backlog.

## What is needed to move the rest to Rust

Author the remaining fixture-data assertions as `gmeow:CompetencyQuestion` cells
in `slices/core/epistemics/tests/competency.ttl` per `docs/SLICE_QA.md`, with the
fixture referenced via `cqDataFile`. Confirm `make slicetest`, then delete this
file and this dossier. No new Rust — the harness exists.
