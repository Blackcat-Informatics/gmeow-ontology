# Issue #1783: fix: make clippy fails on main (build-inputs, action-cache, logic-compile lints)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, quality, rust

## Body

## Problem

`make clippy` (`cargo clippy --all-targets -- -D warnings`) fails on clean `main` (verified at 811b05c05) with the current toolchain. It stops in the first failing crates:

- `crates/build-inputs`: `collapsible_if` / `collapsible_match` (6), `sliced_string_as_bytes` (`src/extract.rs:336`)
- `crates/action-cache/src/executable.rs:110`, `crates/logic-compile/src/projections/get_leg.rs:190`: `large_enum_variant`
- `crates/logic-compile`: `double_must_use` (`frontend/source_admission.rs:266,273`), `needless_bool`-style guard (`:952`), `cloned_ref_to_slice_refs` (`projections/report.tests.rs:364`), `needless_borrow` (`projections/sparql/tests.rs:160,182,235`), `bool_comparison` (`tptp/tests.rs:96`)

With `--cap-lints warn`, the workspace reports 46 distinct lint sites on `main`.

## Acceptance

- Fix (not allow) each lint at its source; `make clippy` passes on the whole workspace.


## Comments (0)

