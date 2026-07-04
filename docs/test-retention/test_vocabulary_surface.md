# Retention: `tests/test_vocabulary_surface.py`

**Category:** Python tool algorithm

## What it tests

Vocabulary-surface integrity gates.

Retained dynamic tests:

- `test_root_imports_are_exactly_the_core_profile` — The root IRI IS the core profile : its owl:imports must equal the tierCore slice set exactly — an extension in the root, or a core slice missing from it, is a gated failure, never silent drift.
- `test_full_profile_imports_every_slice` — <…/gmeow/full> aggregates the root (core) plus every extension — no slice can exist outside its profiles.
- `test_claims_profile_is_genuinely_sub_core` — The slim ruling made measurable: claims ⊂ core, strictly, and carries no extension.
- `test_all_modules_are_in_catalog` — Every ontology/modules/*.
- `test_module_iri_matches_filename` — Each module's owl:Ontology IRI follows its location.
- `test_coverage_fixtures_use_only_declared_terms` — Coverage fixtures must not use undeclared GMEOW vocabulary terms.
- `test_slice_examples_use_only_declared_terms` — Slice worked examples must use only DECLARED GMEOW vocabulary terms.
- `test_slice_source_localizable_literals_are_language_tagged` — Localizable literals in slice source must carry a language tag.
- `test_nonslice_authored_localizable_literals_are_language_tagged` — Authored TTL outside ``slices/`` must also carry language tags.
- `test_docs_examples_use_only_allowed_terms` — User-copyable docs examples must not use unallowlisted gmeow: IRIs.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
