# Retention: `tests/test_notation.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Retained cross-slice and dynamic guards for the notation and symbolic
systems building block (#172).

Retained dynamic tests:

- `test_value_vocabularies_not_subclasses` — No unexpected subclasses of SymbolicSystemKind or NotationUsageRole.
- `test_ambiguous_cases_co_modelable` — Ambiguous systems (MusicXML, MathML, MIDI, ABC) can be co-modeled as both FormalLanguage and NotationSystem through standpoint-indexed claims (Principle 9).

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
