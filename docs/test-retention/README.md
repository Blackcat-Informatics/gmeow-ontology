# Pytest retention dossier

This project is rust-first / python-surface. The pytest suite is being culled to
the smallest possible residue: a test survives **only** if it exercises something
that has no Rust home today and cannot be expressed as a slicetest cell or
`crates/validate` conformance test.

The burden of proof is on *keeping*. Every surviving pytest test (or coherent
file of tests) has a dossier here that states:

1. **What it tests** — the concrete surface under test.
2. **Why it cannot be deleted or moved to Rust today** — the specific blocker.
3. **What is needed to move it to Rust** — the migration that would let it be
   deleted. When that migration lands, the test goes and its dossier is removed.

A test with no dossier is a deletion candidate. A dossier whose "what's needed"
has shipped is a deletion order.

## Retention categories

| Category | Why it survives | Migration that retires it |
|---|---|---|
| Python CLI surface | `gmeow`/`gmeow-dev` are Typer apps; `CliRunner`/subprocess behavior is Python-only | Port the CLI to Rust (`clap`) with integration tests |
| PyO3 seam | tests the binding's marshalling/error-surfacing, which Rust cannot test from the inside | Delete when the Python surface that owns the seam is removed |
| Python tool algorithm | the implementation under test is still live Python (up-projection, transform, projections, mappings, saturate, coverage, crossref, language-tags, music package, gts views/producer) | Port the tool to a Rust crate; cover with crate tests |
| Oracle / Docker orchestration | drives external reasoners (HermiT/ELK/ROBOT) or the rdflib trust-anchor; no Rust twin by design | Retire with the classic-cross-check lane, or reimplement the harness in Rust |
| Static repo guard | AST / filesystem / workflow assertions about the repo itself | Port the static check into a Rust gate |

## Deletion ledger (this branch)

Removed because a Rust artifact already asserts the same behavior:

- Parity goldens (8): coverage, dsl_provenance, fno, language_tags, lint,
  normalize, reasoning, sssom → `crates/validate` (coverage/lint/language_tags/
  gufo/py_dsl), `crates/rdf-core` (fno/canon/sssom), `crates/rdf` (py_sssom).
- `test_logic_counterfactual` → `crates/conformance` runs the worlds-C corpus +
  answer goldens through the same engine; `crates/logic` counterfactual unit tests.
- `test_reasoning_lint` → `crates/validate/src/gufo.rs` (literal twins) + the
  `make validate` reasoning-invariants gate.
- Migration stubs (`test_reference_frames`, `test_profiles`, `test_accessibility`,
  `test_lexicon`) → slicetest cells + `conformance_*.rs`.
- `test_images`, `test_music_analysis/collections/pitch`, `test_procedures` →
  slice `structural.ttl` cells + `conformance_*.rs`.

Constitution `meta:artifact` citations of deleted tests were redirected to the
Rust artifact that now proves the principle.
