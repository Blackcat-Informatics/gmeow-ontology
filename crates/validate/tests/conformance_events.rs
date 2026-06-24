// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_events.py (#867)
//!
//! Migrated tests:
//!   - `test_wellformed_participation_conforms` → `wellformed_participation_conforms`
//!   - `test_malformed_participation_is_flagged` → `malformed_participation_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_event_is_grounded_in_gufo_event`: cross-slice — asserts
//!     `gmeow:Activity rdfs:subClassOf gmeow:Event`; Activity is defined in the
//!     provenance slice, so a fixture-only cell would miss that triple.
//!   - `test_former_event_types_are_individuals_not_classes`: dynamic sweep —
//!     uses `g.subjects(RDFS.subClassOf, GM.LifeEvent)` over the whole merged graph
//!     to catch any GMEOW-prefixed subclass resurrection; narrowing to the events
//!     module would silently weaken the regression guard.
//!   - `test_participation_mediation_axiom_present`: bnode walk — inspects
//!     `owl:Restriction` blank nodes via `g.objects()` + `g.items()` to verify
//!     `someValuesFrom` axioms; bnode list structure is not expressible as a
//!     simple fixture-only test.
//!   - `test_contested_event_claims_coexist_and_validate`: multi-file ABox fixture
//!     loaded dynamically + `run_shacl()` + object sweep.
//!   - `test_schema_role_projection_keys_by_role`: `project_graph()` projection check.
//!   - `test_schema_role_projection_suppresses_withdrawn_participation`: projection.
//!   - `test_schema_fuzzy_time_projects_earliest_bound`: projection bound check.
//!   - `test_ical_vevent_interval_has_start_end_and_location`: projection check.
//!   - `test_ical_vevent_point_has_start_only`: projection check.
//!   - `test_ical_vevent_fuzzy_spans_the_bounds`: projection bound check.
//!   - `test_ical_summary_is_the_event_type_label`: projection label check.
//!   - `test_owl_time_projection_emits_pure_interval_relations`: projection sweep.
//!   - `test_observational_activity_is_subclass_of_activity_and_event`: cross-slice —
//!     asserts `gmeow:Activity rdfs:subClassOf gmeow:Event` (Activity is in provenance).
//!   - `test_observational_activity_chain_on_was_associated_with`: bnode list walk —
//!     inspects `owl:propertyChainAxiom` blank-node list via `g.objects()` + `g.items()`
//!     to verify the exact member order of `generatedObservation` + `vantage`.

mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_events.py ─────────────────────────────────

/// `test_wellformed_participation_conforms` — the `participation-wellformed`
/// fixture (a Participation mediating exactly one event and one participant,
/// with a suppressed correction) must pass SHACL.
///
/// Mirrors:
/// ```python
/// def test_wellformed_participation_conforms() -> None:
///     result = run_shacl(_fixture("participation-wellformed"))
///     assert result.ok, "\n".join(result.errors)
/// ```
#[test]
fn wellformed_participation_conforms() {
    let nt = fixture_as_nt("shapes", "participation-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed Participation fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_participation_is_flagged` — the `participation-malformed`
/// fixture (Participation without event, Participation without participant)
/// must fail SHACL with a violation mentioning `participationEvent` or
/// `participationParticipant`.
///
/// Mirrors:
/// ```python
/// def test_malformed_participation_is_flagged() -> None:
///     result = run_shacl(_fixture("participation-malformed"))
///     assert not result.ok
///     joined = "\n".join(result.errors)
///     assert "participationEvent" in joined or "participationParticipant" in joined
/// ```
#[test]
fn malformed_participation_is_flagged() {
    let nt = fixture_as_nt("shapes", "participation-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed Participation fixture must fail SHACL"
    );
    let joined = violations(&report).join("\n");
    assert!(
        joined.contains("participationEvent") || joined.contains("participationParticipant"),
        "violation message must mention participationEvent or participationParticipant; got: {joined:?}"
    );
}
