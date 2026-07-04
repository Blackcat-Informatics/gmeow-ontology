# Retention: `tests/test_dsl_shacl.py`

**Category:** Python tool algorithm

## What it tests

Tests for RDF-native SHACL validation of the mapping and statement DSL sources.

Retained dynamic tests:

- `TestMappingDslShacl.test_malformed_term_equivalence_shacl_diagnostic` — A TermEquivalence missing alignSubject must fail with a SHACL diagnostic.
- `TestStatementDslShacl.test_malformed_statement_shacl_diagnostic` — A StatementMetadata with both qObject and qObjectLiteral must fail.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
