# Retention: `tests/test_language_tags.py` (+ `test_languages.py` tool parts)

**Category:** Python tool algorithm

## What it tests

`test_language_tags.py`: the BCP-47 selection/filter API (`resolve_lang_input`,
`select_literal`, `filter_literals`, `filter_graph`, `marked`) of
`gmeow_tools.language_tags`. `test_languages.py` (tool parts): `load_tag_map` /
`load_inverse_tag_map` determinism + catalog coverage, `retag_graph_to_internal`,
and the reference-catalog dynamic sweeps (e.g. 184 ISO-639-1 codes present).

## Why it cannot move to Rust today

These exercise live **Python** functions. The classification basics exist in
`crates/validate/src/language_tags.rs`, but the selection/filter/retag API, the
tag-map (and inverse), and the reference-catalog audits are Python-only; the tests
assert that Python output.

## What is needed to move it to Rust

Port the selection/filter/retag API + the tag-map (and inverse) + the
reference-catalog audits to a Rust crate with crate tests, then delete
`test_language_tags.py` (and the tool parts of `test_languages.py`) and this
dossier. The TBox parts of `test_languages.py` move to the languages slice's
`structural.ttl` cells instead.
