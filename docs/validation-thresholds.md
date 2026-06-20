<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Validation thresholds — the #579 gate floors

This is the single source of truth for the four blocking validation gates added
in #579. Consult it **before changing any floor**. Each gate is wired into both
`make check` (the local ELK lane) and CI such that a regression below its floor
fails the build.

## The ratchet rule

Every numeric floor below follows one discipline:

> **Set the floor at-or-just-below the current measured value (with a small
> anti-flake margin). Raise it as the metric improves. NEVER lower it.**

A floor is a contract: it only ever moves in the direction of more coverage /
more recall. If a change would drop a metric below its floor, the change is wrong
— not the floor. If you genuinely improve a metric, raise the floor to lock the
gain in. Lowering a floor requires an explicit, reviewed justification and should
be treated as a regression of the contract, not routine maintenance.

## The four gates

### 1. SHACL validation (authoritative, no numeric floor)

- **Floor:** binary — zero SHACL violations. The `gmeow_shacl` Rust validator is
  the sole SHACL engine (pySHACL and the #578 dual-run were deleted in this PR).
- **Measures:** every authored example + the DSL graphs are validated against the
  committed/generated SHACL shapes; any violation is an error and exits non-zero.
- **Command:** `gmeow-dev validate` (Makefile `validate` target / CLI `validate`).
- **Where it lives:** `src/gmeow_tools/validate.py` — `validate_all()` runs
  `run_shacl(...)` through the `gmeow_shacl` extension; violations partition to
  `errors`. Proven by `tests/test_shacl_engine.py::test_violation_partitions_to_errors_with_stable_line`.
- **Ratchet:** not numeric — the contract is and stays "zero violations".

### 2. Vendored-entity coverage (hard class + predicate floors)

- **Floors:** class `--min-class 0.92`, predicate `--min-predicate 0.85`.
- **Measured from:** class coverage `0.9206`, predicate coverage `0.8535`
  (313/27 classes, 932/160 predicates covered/gap). The floors sit just below the
  measured values with anti-flake margin.
- **Command:** `gmeow-dev coverage --gaps --min-class 0.92 --min-predicate 0.85`
  (Makefile `coverage` target). Omitting the `--min-*` flags makes it report-only,
  so CI and the Makefile MUST pass them for the gate to block.
- **Where the floors live:**
  - Makefile `coverage` target (`coverage --gaps --min-class 0.92 --min-predicate 0.85`).
  - `.github/workflows/ci.yml` — `ontology` job, "Vendored-entity coverage — hard
    class/predicate floors (#579)" step.
  - Enforcement: `src/gmeow_tools/cli_dev.py` `coverage()` command
    (the `min_class` / `min_predicate` comparison that raises `_fail`).
- **Ratchet:** as GMEOW grows to cover more vendored classes/predicates, raise
  `0.92` / `0.85` toward the new measured values.

### 3. Slice-example coverage (every slice ships a validating example)

- **Floor:** binary — every `slices/*/*/manifest.ttl` slice must ship at least one
  `examples/*.ttl` file, and that example must validate (SHACL clean).
- **Measures:** presence of `examples/*.ttl` per slice; the examples are then SHACL-
  validated by the same `validate` run.
- **Command:** `gmeow-dev validate` (the same SHACL gate above).
- **Where it lives:** `src/gmeow_tools/validate.py` — `check_example_coverage()`,
  called from `validate_all()`; a missing example appends a hard error
  ("`slice <name>: no examples/*.ttl — every slice must ship at least one
  validating example (#579)`").
- **Ratchet:** not numeric — the contract is "100% of slices have a validating
  example" and only ever stays at 100%.

### 4. Transpile / projection recall (hard aggregate floor)

- **Floor:** `--min-recall 60` (the Makefile `ACCEPTANCE_MIN_RECALL` variable).
- **Measured from:** corpus-aggregate round-trip recall `64.28%` (pooled
  Σ recovered / Σ addressable across the `external/` snapshots). Floor set to 60
  with anti-flake margin. The **per-file** round-trip/coverage gates stay
  scoreboard-soft (no ~100% demand); only this pooled aggregate is hard.
- **Command:** `gmeow-dev acceptance --min-recall 60` (Makefile `acceptance`
  target). Omitting `--min-recall` makes it report-only.
- **Where the floor lives:**
  - Makefile `ACCEPTANCE_MIN_RECALL ?= 60` (consumed by the `acceptance` target).
  - `.github/workflows/ci.yml` — `ontology` job, "Transpile acceptance — hard
    aggregate recall floor (#579)" step.
  - Enforcement: `src/gmeow_tools/cli_dev.py` `acceptance()` command
    (`min_recall` aggregate comparison that exits 1).
- **Ratchet:** as transpile fidelity improves, raise `60` toward the new measured
  aggregate.

## Gate → make-target → CI-job map

| Gate | Make target (in `make check`) | CI job + step |
|---|---|---|
| SHACL | `validate` | `ontology` → "Validate (syntax, lint, SHACL, DSL SHACL)" (also exercised by `python`, `python-heavy`) |
| Vendored-entity coverage | `coverage` (`--min-class 0.92 --min-predicate 0.85`) | `ontology` → "Vendored-entity coverage — hard class/predicate floors (#579)" |
| Slice-example | `validate` (`check_example_coverage`) | `ontology` → "Validate (…)" (same `validate` invocation) |
| Transpile recall | `acceptance` (`--min-recall 60`) | `ontology` → "Transpile acceptance — hard aggregate recall floor (#579)" |

All four also run in the parallel `make check` batch
(`lint validate … coverage acceptance …`), so the local ELK lane blocks on every
floor too.

## Validation cache decision (#579)

**Decision: KEEP the `.cache/validate` layer in this PR.** The cache (keyed on the
validation sources + SHACL shapes, see `src/gmeow_tools/validate.py`
`_VALIDATION_CACHE_DIR` and the `actions/cache` "Cache validation results" step in
CI) avoids re-running SHACL over unchanged inputs. Removing it now risks a CI-time
regression with no offsetting benefit while the Rust revalidation path is still
new. Re-assessing whether Rust-native revalidation is fast enough to drop the cache
is a tracked follow-up, not a blocker for this PR. Do **not** remove the cache as
part of #579.

## CI build cost (#579)

`gmeow_validate` was added as a **3rd nightly-built Rust extension** alongside
`gmeow_shacl` and `gmeow_logic` across the validation jobs that exercise the
validation path: `python`, `python-heavy`, and `ontology`. Qualitatively it compiles next to the existing crates in
the same `maturin develop` phase, so the incremental cost is one more crate build
on the nightly toolchain; wheel/dependency caching via the existing
`Swatinem/rust-cache` step is already in place to amortize it across runs. No
micro-benchmark of CI minutes was taken — the cost is "one additional crate on top
of two already-built ones, behind the existing rust-cache".
