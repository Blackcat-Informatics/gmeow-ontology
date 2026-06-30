# Retention: `tests/test_coreference.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Universal identity/coreference guards (#74). Only the whole-graph banned-IRI
absence sweep (`test_no_preferred_or_primary_coreference_terms`) remains. The
`test_schema_sameas_projection_requires_exact_authority_match` projection check
was removed (#1092 / F5): the `schema-org` `sameAs` projection is now enforced by
the native Rust SPARQL lowering and its byte-iso parity oracle, so the Python
`project_graph` surface was the redundant dual authority.

## Why it cannot move to Rust today

Structural / competency / cross-slice invariants over the module (or merged) graph — ontology *shape*, not Python logic.

## What is needed to move it to Rust

Author the assertions as slicetest cells in the **owning** slice (`structural.ttl` MUST/MUST-NOT, `competency.ttl` ASK/SELECT) per `docs/SLICE_QA.md`; cross-slice subjects go in the slice that *defines* the term. Confirm `make slicetest`, then delete this file. No new Rust — the harness exists.
