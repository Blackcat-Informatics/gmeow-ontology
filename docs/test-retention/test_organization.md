# Retention: `tests/test_organization.py`

**Category:** Merged-graph guard

## What it tests

Standpoint + fixture guards for the organization module.

Retained dynamic tests:

- `test_contested_membership_coexists` — Two contradictory standpoint-indexed memberOf claims load, SHACL-pass, and are BOTH retained — neither is the ground truth.
- `test_contested_succession_coexists` — Two standpoint-indexed subOrganizationOf claims post-merger coexist.
- `test_withdrawn_recognition_suppressed_not_deleted` — A closed StandpointTenure with displayable false is retained.
- `test_no_preferred_or_primary_org_term` — Principle 9: no single slot to win — organizations mints no preferred/primary selector for a contested member, successor, or recognition.
- `test_post_seat_independent_of_holder` — A Post exists without any Membership filling it — the vacancy case.
- `test_post_successive_holders` — Two Memberships may fill the same Post in succession.
- `test_site_location` — An organization has sites with typed locations.
- `test_change_event_entailments` — Merger and split events link predecessor and successor organizations.
- `test_wellformed_legal_identifier_passes` — An organization with reified Identifier nodes (value + scheme) passes.
- `test_change_event_type_values_exist` — The multi-org change event type vocabulary is seeded (cross-slice).

## Why it cannot be deleted or moved to Rust today

- Standpoint coexistence fixtures (`run_shacl` plus `g.objects()` graph-content assertions that cannot be expressed as pure SHACL checks).
- The whole-ontology Principle-9 banned-term sweep (uses the full merged graph; narrowing to `scopeModule` would miss cross-slice violations).
- Graph-manipulation fixture tests (`wellformed legal-identity` — requires `g.remove()` before validation).
- Cross-slice `EventType` seeds (`eventTypeMerger`/`Split`/... live in `slices/core/events/module.ttl`, not here).
