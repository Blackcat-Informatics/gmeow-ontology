# Retention: `tests/test_email_thread_subject.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Structural guards for threadSubject and subjectPrefix. Issue #138.

Retained dynamic tests:

- `test_fixture_has_thread_subject_and_prefix` — The coverage fixture must include a Thread with threadSubject and a reply message with subjectPrefix.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
