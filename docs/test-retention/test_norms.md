# Retention: `tests/test_norms.py`

**Category:** Cross-slice file-load guard

## What it tests

`test_graft_axioms_live_extension_side_only` loads `slices/core/rights/module.ttl`
as a separate graph and asserts that no norms-extension IRI appears there — the
rights graft is asserted only in the norms extension slice.

## Why it cannot move to Rust today

It is a file-load / cross-graph absence check, not a module-scoped TBox ASK
assertion. The declarative test-DSL structural assertions are evaluated per
slice module, not across separately parsed files.

## What is needed to move it to Rust

Either extend the slicetest harness with a cross-file absence primitive, or
collapse this into a static repo guard (e.g. a `gmeow-dev` command that parses
the two modules separately). Until then it stays as the single pytest residue
for the norms cluster.
