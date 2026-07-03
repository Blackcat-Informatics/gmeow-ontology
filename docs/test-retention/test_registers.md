# Retention: `tests/test_registers.py`

**Category:** Dynamic graph sweep + mutated fixture query

## What it tests

- `test_no_primary_persona_machinery`: scans the merged ontology for any GMEOW
  term whose local name starts with a banned primary/preferred persona/register
  prefix (Principle 9 guard).
- `test_divergence_query_surfaces_legal_divergence`: mutates the registers
  wellformed fixture with an extra private-only norm, verifies SHACL still
  conforms, and checks the divergence query reports the injected norm only for
  the private persona.

## Why it cannot move to Rust today

The first test is a dynamic prefix sweep over all subjects in the merged graph.
The second is a hybrid SHACL + SPARQL test over a mutated fixture graph, which
cannot be expressed as a static module-scoped cell.

## What is needed to move it to Rust

The sweep could become a vocabulary-stem SHACL constraint; the hybrid test
could be split into a static counter-example fixture and a competency question
with an overlaid data file. Both require test-harness extensions, so they stay
in pytest for now.
