# CI and local check slowdown notes

Date: 2026-06-15
Worktree: `.worktrees/check-slowdowns`
Branch: `perf/ci-check-slowdowns`
Base: `origin/main` at `c929acedd6`

This is an exploration note, not an optimization patch. It records the current
slow and hanging paths in CI plus local `make check` / `make test` evidence.

## Executive summary

The current CI bottleneck is the `python` job. Recent runs show pytest reaching
about 95% and then emitting no further progress for hours until cancellation.
The native `gmeow_logic` extension is not the cause: it builds in about one
minute in CI, and the focused native test module completes locally in under a
second after `make native-py`.

The current local and CI test hotspots are repeated full-artifact builds:

- `tests/test_ontology_docs.py` rebuilds the full ontology docs tree in nearly
  every test. Full module timing with four workers was 3:12 locally.
- `tests/test_gts_gen.py` rebuilds the full GTS snapshot five times across
  three determinism/drift assertions. Focused module timing with four workers
  was 3:05 locally.
- `tests/test_compile_no_drift.py` and
  `tests/test_suppression_conformance.py` repeatedly reload or regenerate the
  same mapping DSL / projection artifacts.
- `make check` runs `sync`, `validate`, `constitution-check`, and
  `lint-alignment`, then `compliance-report` runs those in-process gates again.
  In CI, the `ontology` job shows `validate` around 18 minutes and
  `compliance-report` around 16-17 minutes on successful runs.

## CI evidence

Recent successful CI runs before the current drift failures showed these
critical-path jobs:

- Run `27524977688`: `python` 37:12, with `Unit tests (pure Python)` 36:23.
- Run `27524977688`: `ontology` 36:48, with `Validate` 16:23 and
  `Compliance report` 17:00.
- Run `27524977688`: the external OWL 2 DL reasoning lane 15:48 (since removed; the
  native, in-process reasoning cross-check replaced it).
- Run `27524977688`: `statements` 6:15, mostly Jena image build.

Current `origin/main` run `27536210920` failed ontology drift after validate,
but the parallel `python` job kept running:

- `Unit tests (pure Python)` started at `2026-06-15T09:17:54Z`.
- Pytest reached 95% at `2026-06-15T10:02:25Z`.
- No further pytest progress appeared before the job was cancelled at
  `2026-06-15T15:16:26Z`.

That points to a small long-tail or hanging test set after most tests have
already finished. The most plausible current tail is full snapshot/docs
generation under xdist, not the Rust/PyO3 extension.

## Local evidence

Local host timing was contaminated by unrelated CPU-bound processes in other
worktrees, so wall-clock numbers should be treated as directional. The test
duration reports are still useful for identifying repeated work.

`make test-fast` with duration reporting selected 2,396 tests and produced:

- 5 failures on current `origin/main` drift:
  - `tests/test_acceptance.py::test_run_acceptance_over_a_real_snapshot`
  - `tests/test_up_projection_audit.py::test_real_data_baseline_is_sane`
  - `tests/test_vocabulary_surface.py::test_root_imports_are_exactly_the_core_profile`
  - `tests/test_ontology_docs.py::test_external_ontology_catalog_has_specific_descriptions`
  - `tests/test_gts_gen.py::test_committed_snapshot_matches_a_fresh_build`
- Slowest calls:
  - `tests/test_gts_gen.py::test_double_build_is_byte_identical`: 127.63s
  - `tests/test_gts_gen.py::test_cross_hash_seed_builds_are_byte_identical`: 113.09s
  - `tests/test_ontology_docs.py::test_external_ontology_catalog_has_specific_descriptions`: 82.70s
  - `tests/test_compile_no_drift.py::test_committed_artifacts_match_dsl`: 78.55s
  - former Python narrow-waist behavioral seal: 61.11s
  - several mapping compiler and suppression conformance tests at 39-52s.

Focused module timings:

- `tests/test_ontology_docs.py`, four xdist workers: 20 passed, 1 failed in
  192.60s. Individual docs builds were usually 27-52s; the deterministic docs
  test was 80.76s because it builds twice.
- `tests/test_gts_gen.py`, four xdist workers: 9 passed, 1 failed in 185.04s.
  The three expensive assertions were 101.58s, 82.10s, and 52.72s.
- `tests/test_compile_no_drift.py`, four xdist workers: 8 passed in 45.97s.
  The drift test was 39.48s, and some EDOAL determinism cases were 23-25s.
- `tests/test_suppression_conformance.py`, four xdist workers: 84 passed in
  33.19s. The structural branch-guard cases reload/re-render per profile.
- After `make native-py`, `tests/test_logic_engine.py` completed 13 tests in
  0.33s, so the CI Python hang is not the native materialize test module.

## Likely fixes

1. Split Python CI into separate jobs for normal fast tests, ontology docs tests,
   and GTS bundle/snapshot tests. This gives branch protection earlier failure
   visibility and prevents one tail from hiding the rest of the suite.

2. Add an explicit timeout to the Python pytest step. Six-hour silent tails are
   not useful feedback. A 45-60 minute timeout would preserve failure evidence
   while stopping wasted CI capacity.

3. Cache full ontology-doc output inside `tests/test_ontology_docs.py` with a
   module-scoped fixture, then point the individual assertions at the same tree.
   Keep the deterministic two-build test as the only deliberate second build.

4. Reduce GTS snapshot rebuilds. Build once in a fixture for content assertions,
   keep one dedicated determinism test, and avoid rebuilding full ontology docs
   inside every snapshot build unless the test specifically covers doc bundling.

5. Keep structural suppression-guard checks in the Rust slice emitter tests so
   parsed DSL, suppression vocabulary, and branch rendering happen in one native
   pass instead of repeatedly in Python.

6. Keep mapping-compiler parity on the native generator DAG (`make
   sync`) and avoid reintroducing a duplicate Python parser/emitter
   test surface.

7. Stop rerunning full gates inside `compliance-report` during routine local
   checks. `make compliance-report` should emit the RDF from already-run gate
   outcomes using `--from-passing-check`; `make compliance-report-full` should
   remain the explicit release/report-regeneration path that reruns gates.

8. Add phase timing to `validate_all()`. The code path currently does syntax,
   sameAs, merged graph lints, full SHACL, 61 per-example SHACL validations,
   and DSL SHACL. The CI job only exposes one 18-minute `Validate` step, which
   is not enough to see which phase is growing.

## Current failures observed during profiling

The worktree is based on current `origin/main`, which was already failing drift
and generated-output checks during this exploration. These failures are not
introduced by this report, but they make full local `make check` unsuitable as a
clean performance baseline until the generated artifacts are refreshed or the
mainline drift is resolved.
