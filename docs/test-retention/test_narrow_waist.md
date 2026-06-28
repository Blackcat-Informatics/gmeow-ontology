# Retention: `tests/test_narrow_waist.py`

**Category:** Static repo guard

## What it tests

The narrow-waist seal: GTS is the only exit for data. By AST-parsing the source,
it proves `export.py` imports no rdflib and no canonical loaders, and that the
public CLI does not reimplement GTS subcommands. A static architectural invariant
about the repository's own code.

## Why it cannot move to Rust today

It statically analyzes **Python source** (imports, call sites) with Python's
`ast`. The subject of the assertion is the Python codebase's import graph; the
check must parse Python to be meaningful. No Rust test inspects Python ASTs.

## What is needed to move it to Rust

Reimplement the guard as a Rust gate that parses the Python sources (e.g. with a
Python-aware parser) or, better, retire it once the data-exit surface is fully
Rust and the narrow-waist is enforced by crate-layering rules
(`crates/validate` crate-layering lint) rather than by scanning Python imports.
