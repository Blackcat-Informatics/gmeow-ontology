# Retention: `tests/test_crossref.py` (+ `test_crossref_parity.py`)

**Category:** Python tool algorithm

## What it tests

The Crossref deposit-XML generator and DOI lint (`gmeow_tools.crossref`): that the
deposit is well-formed, carries the DOI and resource, validates against the
schema, and that `doi-lint` catches malformed records. `test_crossref_parity.py`
guards byte-identity of the emitted XML.

## Why it cannot move to Rust today

`build_deposit_xml` / `lint_deposit` are live **Python**. There is no Rust twin
asserting the byte-for-byte deposit XML — the parity test is currently the *only*
gate on serialization fidelity (this is why it was not deleted with the other
`*_parity` goldens, which all had Rust twins).

## What is needed to move it to Rust

Port the deposit-XML emitter + DOI lint to a Rust crate with a golden test pinning
the byte-exact XML, then delete both files and this dossier. (Crossref is a
registered generator, so the loss/preservation story rides the normal generator
machinery once Rust-native.)
