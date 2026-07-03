# Retention: `tests/test_rubrics.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The rubrics facility (#353, EPIC #348), in the norms slice.

Retained dynamic tests:

- `test_no_preferred_assessment_machinery` — No preferredScore / canonicalAssessment selectors (Principle 9): two judges disagreeing are two coexisting cells.
- `test_two_judges_disagree_without_contradiction` — The LLM-judge doctrine in fixture form: one chunk, two vantages, two scores — both cells stand.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
