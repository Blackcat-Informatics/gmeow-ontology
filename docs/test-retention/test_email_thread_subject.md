# Retention: `tests/test_email_thread_subject.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for threadSubject and subjectPrefix.

Retained dynamic tests:

- `test_fixture_has_thread_subject_and_prefix` — The coverage fixture must include a Thread with threadSubject and a reply message with subjectPrefix.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
