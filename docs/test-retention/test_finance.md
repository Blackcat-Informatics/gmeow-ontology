# Retention: `tests/test_finance.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Retained dynamic guards for the finance module (#64).

Retained dynamic tests:

- `test_monetary_amount_is_entity`
- `test_currency_vocab_is_open_values_not_subclasses`
- `test_monetary_value_is_functional`
- `test_currency_is_functional`
- `test_currency_is_subproperty_of_has_reference_frame`
- `test_currency_frames_have_realm_currency`
- `test_no_transaction_subclass_explosion`
- `test_transaction_type_vocab_is_open_values`
- `test_transaction_uses_participation_not_subproperty`
- `test_asset_type_vocab_is_open_values`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
