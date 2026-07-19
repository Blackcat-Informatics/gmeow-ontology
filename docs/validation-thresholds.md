<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Validation thresholds — gate floors

This is the single source of truth for the four blocking validation gates.
Consult it **before changing any floor**. Each gate is wired into both
`make check` (the local validation batch — these four floors run alongside the separate
one-closure `reason-verify`) and CI such that a regression below its
floor fails the build.

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
  the sole SHACL engine.
- **Measures:** every authored example + the DSL graphs are validated against the
  committed/generated SHACL shapes; any violation is an error and exits non-zero.
- **Command:** `gmeow-dev validate` (Makefile `validate` target / CLI `validate`).
- **Where it lives:** Rust-native validation orchestration in
  `crates/validate/src/validate_all.rs`; the Python CLI is only the surface.
  The build DAG also runs `crates/pipeline/src/stages/validate.rs` over the
  loaded authored validation graph, emits
  `generated/diagnostics/shacl.{json,sarif,html,nq}`, and folds the SHACL report
  into `generated/dist/gmeow.gts`.
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
    class/predicate floors" step.
  - Enforcement: the `gmeow-dev coverage` command (`crates/gmeow-dev-cli`)
    (the `min_class` / `min_predicate` comparison that fails the gate).
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
  validating example`").
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
    aggregate recall floor" step.
  - Enforcement: the `gmeow-dev acceptance` command (`crates/gmeow-dev-cli`)
    (`min_recall` aggregate comparison that exits 1).
- **Ratchet:** as transpile fidelity improves, raise `60` toward the new measured
  aggregate.

## Gate → make-target → CI-job map

| Gate | Make target (in `make check`) | CI job + step |
|---|---|---|
| SHACL | `validate` | `ontology` → "Validate (syntax, lint, SHACL, DSL SHACL)" (also exercised by `python`, `python-heavy`) |
| Vendored-entity coverage | `coverage` (`--min-class 0.92 --min-predicate 0.85`) | `ontology` → "Vendored-entity coverage — hard class/predicate floors" |
| Slice-example | `validate` (`check_example_coverage`) | `ontology` → "Validate (…)" (same `validate` invocation) |
| Transpile recall | `acceptance` (`--min-recall 60`) | `ontology` → "Transpile acceptance — hard aggregate recall floor" |

All four also run in the parallel `make check` batch
(`lint validate … coverage acceptance …`), so the local validation batch blocks on every
floor too (the aggregate `reason-verify` runs beside it; focused reasoning targets remain
available for diagnosis).

## Validation cache decision

**Decision: KEEP the `.cache/validate` layer.** The Rust validation cache is keyed
on validation sources, SHACL shapes, and toolchain versions, avoiding repeated
SHACL work over unchanged inputs. The pipeline-stage report is separately
content-addressed by the DAG and compared through strict `sync`.

## CI build cost

`gmeow_validate` was added as a **3rd nightly-built Rust extension** alongside
`gmeow_shacl` and `gmeow_logic` across the validation jobs that exercise the
validation path: `python`, `python-heavy`, and `ontology`. Qualitatively it compiles next to the existing crates in
the same `maturin develop` phase, so the incremental cost is one more crate build
on the nightly toolchain; wheel/dependency caching via the existing
`Swatinem/rust-cache` step is already in place to amortize it across runs. No
micro-benchmark of CI minutes was taken — the cost is "one additional crate on top
of two already-built ones, behind the existing rust-cache".
