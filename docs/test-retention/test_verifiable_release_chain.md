# Retention: `tests/test_verifiable_release_chain.py`

**Category:** Static repo guard

## What it tests

Structural + fixture + competency tests for the verifiable release chain.

Retained dynamic tests:

- `test_build_activity_is_activity` — Retained dynamic test.
- `test_builder_is_software_agent` — Retained dynamic test.
- `test_build_properties_exist` — Retained dynamic test.
- `test_release_doi_property_exists` — Retained dynamic test.
- `test_build_event_type_seeded` — Retained dynamic test.
- `test_fixture_signed_commit` — Retained dynamic test.
- `test_fixture_signed_tag` — Retained dynamic test.
- `test_fixture_release_with_doi` — Retained dynamic test.
- `test_fixture_build_activity` — Retained dynamic test.
- `test_fixture_slsa_attestation` — Retained dynamic test.
- `test_fixture_cosign_signature` — Retained dynamic test.
- `test_fixture_rekor_entry` — Retained dynamic test.
- `test_fixture_swhid_on_commit` — Retained dynamic test.
- `test_query_key_that_signed_commit` — Which key signed the commit that the release tag points to?.
- `test_query_build_that_produced_artifact` — Which build produced the artifact with this SWHID?.
- `test_query_rekor_entry_for_attestation` — Is there a Rekor entry for the attestation of this release?.

## Why it cannot be deleted or moved to Rust today

Filesystem, AST, or workflow assertion about the repository itself; not expressible as a module-scoped slice-test cell.
