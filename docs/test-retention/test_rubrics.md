# Retention: `tests/test_rubrics.py`

**Category:** Dynamic graph sweep + fixture value check

## What it tests

- `test_no_preferred_assessment_machinery`: scans the merged ontology for any
  GMEOW term whose local name starts with a banned preferred/primary/canonical
  assessment prefix (Principle 9 guard).
- `test_two_judges_disagree_without_contradiction`: loads the rubrics wellformed
  fixture and checks that two co-equal `gmeow:Assessment` cells with different
  scores coexist.

## Why it cannot move to Rust today

The first test is a dynamic prefix sweep over all subjects in the merged graph;
the second reads literal score values from a fixture. Neither is expressible as
a module-scoped SPARQL ASK/SELECT cell.

## What is needed to move it to Rust

The dynamic sweep could become a SHACL shape over the merged vocabulary once
GMEOW has a stable closed set of assessment-related term stems; the fixture
check could move to a slice-local competency fixture with explicit expected
rows. Both are larger test-infrastructure changes, so they stay in pytest for
now.
