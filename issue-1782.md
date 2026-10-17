# Issue #1782: fix: reason::artifacts closure test fixture lacks native execution evidence (red on main)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, logic, rust

## Body

## Problem

`gmeow-logic reason::artifacts::tests::closure_emits_triple_and_reifier_with_provenance` fails on clean `main` (verified at 811b05c05):

```
ReasoningResult invariant violated: ... "world consistency result is missing its native execution evidence"
(crates/logic/src/result.rs:1079)
```

The test builds a `ReasoningResult` fixture without the native execution evidence that the result invariant now requires.

## Acceptance

- The fixture carries real native execution evidence (or is produced through the native reasoner), and the invariant stays enforced.
- The test passes; `make check` passes.


## Comments (3)

### paudley — 2026-10-15T18:01:37Z

# Artifact result fixture repair

The issue requires real native evidence for the closure regression, continued invariant enforcement, and a passing full gate. Principles 1, 4, 7, and 18 apply. No production result contract changes are planned.

## Task 1: Repair artifact fixtures

Use a tiny, explicit native reasoner input for closure/reifier provenance and status integration tests. Use real native execution for the typed-object verifier test. Keep deliberately malformed, modal, duplicate-proof and serializer-only fixtures explicitly scoped to conclusion projection with no world-consistency claim. Never attach unrelated execution evidence to invented rows. Check every shared-helper caller.

Acceptance: make fmt; make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & test(reason::artifacts::tests)'; relevant result invariant regressions; make lint.

## Task 2: Qualify, publish, and review

Run the required full qualification after focused repairs settle, reuse hosted evidence where applicable, commit/push normally and publish the exact results. Obtain independent review before readiness or merge; no independent reviewer tool is currently available, so no such review is claimed. Preserve selected Stage evidence for ghprsq capture. No issue completion or merge before required gates.

Prior art: issue 1782 has no prior attempts or comments; merged dependency PR 1784 cites it but does not close the fixture defect. Brief coverage is retained in prior-art.md. Repository-local .stage evidence is excluded from source under the existing local-tool-state policy, retained for explicit evidence archival.


### paudley — 2026-10-15T18:20:52Z

Fixture repair is committed and pushed as 784e499e7 (Principles 1, 4, 7 and 18).

The named closure regression now executes the native reasoner on a tiny taxonomy and derives the receipt from its actual result. The status tests use actual native consistency and contradiction results. Typed literal, direction and recursive quoted-term tests now exercise a real Horn copy before artifact emission. Malformed and duplicate-proof serializer fixtures no longer impersonate world-consistency decisions or borrow unrelated execution evidence.

Verified: make fmt; make lint; 16 corpus-free artifact tests, including rejection of missing or mismatched execution evidence. Nextest run f4ae76be-babf-47ad-8618-90772fec2698 passed. Production result invariants and compiler/runtime settings are unchanged.

Still required: both authenticated corpus-verifier cases, full make check and independent review. The corpus-free runner correctly refused absent producer-selected law fixtures; those cases remain in the suite. The exact repair will be qualified with the combined reasoning branch, avoiding a second producer run. This issue remains open; no full-gate or merge-readiness claim is made.


### paudley — 2026-10-16T22:58:20Z

# Current artifact regression qualification

The fixture repair is already integrated on main as 13a44f7a9, with later artifact
corrections in b797a315e and eca80ae00. The old standalone 784e499e7 commit is not
an ancestor because the repair was integrated under another commit identity.

A fresh selection on the current combined proof branch passed all 16 synthetic
artifact cases in 0.278s, including closure_emits_triple_and_reifier_with_provenance
and rejection of missing or mismatched native execution evidence. Native typed,
directional and recursively quoted RDF 1.2 artifacts also passed. Command:
make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & test(reason::artifacts::tests::) & !test(corpus_)'.
Log: issue-1782-current-artifact-tests.log.

The two authenticated corpus-verifier cases and full gate remain unqualified.
Current pipeline prerequisite qualification is ongoing; no test produced corpus
fixtures, no invariant was weakened, and the issue remains open. Cleanup dry-run
refused the old remote branch as not ancestry-merged. No force or deletion was used.


