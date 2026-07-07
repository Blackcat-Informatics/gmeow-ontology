// SPDX-License-Identifier: AGPL-3.0-only

//! The Python-fn → native-twin reconciliation manifest for the graph-API
//! traversal migration (the rdflib `load_merged_graph` traversal cluster).
//!
//! Sibling of [`super::migration_manifest`] (the frozen SPARQL-cluster record);
//! kept separate so each migration's coverage proof is independent.
//!
//! One [`ManifestRow`] per Python test fn across the 35 files in scope, captured
//! from `grep 'def test_'` at the migration merge-base BEFORE deletion — the LEFT
//! side (`python_file`, `python_fn`, `case_count`) is the authoritative inventory,
//! anchored to git history (not re-derivable once the `.py` are deleted). The
//! per-file `def test_` lists are pinned verbatim in [`EXPECTED`] below.
//!
//! The RIGHT side is the reconciliation state: each migration task FILLS a
//! [`TwinState::Pending`] row by editing it to [`TwinState::Twin`] with a locator
//! AND setting the row's final [`ObligationKind`]. The coverage proof is *typed*:
//! [`twin_kind_matches_locator_surface`] asserts a `TBoxStructural` twin lives in a
//! `structural.ttl` cell, an `ABoxWitness` in an `example-conformance.ttl` cell, and
//! a `QueryBehavioral`/`ProjectionPreservation` twin in a Rust `*.rs::fn` — so a
//! twin cannot claim a dogfooded home it does not actually occupy.
//!
//! [`migration_is_reconciled_and_complete`] is the finalization gate: zero
//! `Pending`, zero `Dropped`, per-file counts matching [`EXPECTED`]. The `kind` on a
//! `Pending` row is provisional (unchecked) and is finalized when the row is twinned.

/// The reconciliation state of a Python test fn during the native migration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TwinState {
    /// A native twin exists, at this locator: a `…/tests/structural.ttl#cell` or
    /// `…/tests/example-conformance.ttl#cell` IRI for cell twins, or a
    /// `file.rs::fn`-style path for Rust twins.
    Twin(&'static str),
    /// No twin yet — a later migration task fills this slot.
    Pending,
    /// Behaviour intentionally retired (not ported), never to get a twin. Target
    /// for this migration is ZERO drops (user ruling: migrate everything).
    Dropped,
}

/// The semantic obligation kind a twin discharges — the four kinds the codebase
/// already names (the three slicetest cell types plus the projection surface).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObligationKind {
    /// TBox-structural invariant → `structural.ttl` ASK cell.
    TBoxStructural,
    /// ABox-witness (a bound example conforms/violates a shape) → `example-conformance.ttl`.
    AboxWitness,
    /// Query-behavioral (count/enumeration/SPARQL-over-fixture/parse) → Rust conformance.
    QueryBehavioral,
    /// Projection-preservation (a `project_graph` output assertion) → Rust conformance.
    ProjectionPreservation,
}

/// One `(python_file, python_fn) → reconciliation state` row.
#[derive(Clone, Copy, Debug)]
pub struct ManifestRow {
    /// Repo-relative path of the Python test file (as it existed at the merge-base).
    pub python_file: &'static str,
    /// The Python test fn name (`test_*`).
    pub python_fn: &'static str,
    /// Logical case count — 1 for a plain fn, or the parametrize cardinality for a
    /// `@pytest.mark.parametrize` fn (so the gate cannot pass while narrowing a
    /// parametrized fn to a single case).
    pub case_count: usize,
    /// The obligation kind this fn's invariant belongs to (provisional while `Pending`,
    /// authoritative once `Twin`).
    pub kind: ObligationKind,
    /// Reconciliation state (later tasks edit `Pending` → `Twin(..)`).
    pub state: TwinState,
}

use ObligationKind::*;

/// A `TBoxStructural` row a later task must supply (case_count = 1).
const fn t(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count: 1,
        kind: TBoxStructural,
        state: TwinState::Pending,
    }
}
/// An `AboxWitness` row a later task must supply (case_count = 1).
const fn a(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count: 1,
        kind: AboxWitness,
        state: TwinState::Pending,
    }
}
/// A `QueryBehavioral` row a later task must supply (case_count = 1).
const fn q(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count: 1,
        kind: QueryBehavioral,
        state: TwinState::Pending,
    }
}
/// A `ProjectionPreservation` row a later task must supply (case_count = 1).
const fn p(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count: 1,
        kind: ProjectionPreservation,
        state: TwinState::Pending,
    }
}
/// A `ProjectionPreservation` row with an explicit parametrize cardinality.
const fn pn(python_file: &'static str, python_fn: &'static str, case_count: usize) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count,
        kind: ProjectionPreservation,
        state: TwinState::Pending,
    }
}

/// A row with a landed native twin at `locator`, of obligation `kind`. Later tasks
/// call this in place of the pending ctors when they land a twin.
#[allow(dead_code)]
const fn twin(
    python_file: &'static str,
    python_fn: &'static str,
    case_count: usize,
    kind: ObligationKind,
    locator: &'static str,
) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        case_count,
        kind,
        state: TwinState::Twin(locator),
    }
}

/// The full reconciliation manifest — 113 rows across the 35 in-scope files.
pub const MANIFEST: &[ManifestRow] = &[
    // ── slices/core/temporal/tests/test_temporal_frame.py (7) ──────────────────
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_temporal_frame_subclasses_reference_frame",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saTemporalFrameSubclassesReferenceFrame",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_temporal_frame_component_classes_exist",
        1,
        QueryBehavioral,
        "conformance_temporal.rs::temporal_frame_component_classes_exist",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_temporal_frame_seed_individuals",
        1,
        QueryBehavioral,
        "conformance_temporal.rs::temporal_frame_seed_individuals",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_temporal_frame_utc_gregorian_exists_with_components",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saTemporalFrameUTCGregorianComponents",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_frame_time_scale_is_functional",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saFrameTimeScaleIsFunctional",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_has_temporal_frame_is_subproperty_of_has_reference_frame",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saHasTemporalFrameSubpropertyOfReferenceFrame",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_frame.py",
        "test_in_temporal_frame_is_subproperty_of_has_reference_frame",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saInTemporalFrameSubpropertyOfReferenceFrame",
    ),
    // ── slices/core/temporal/tests/test_temporal_measurement.py (13) ────────────
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_temporal_measurement_and_dating_method_exist",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saTemporalMeasurementAndDatingMethodExist",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_seed_dating_methods_exist",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saSeedDatingMethodsExist",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measurement_method_is_functional",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasurementMethodIsFunctional",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measured_age_range_is_decimal",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasuredAgeRangeIsDecimal",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measurement_determinacy_links_to_determinacy",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasurementDeterminacyRangeIsDeterminacy",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_temporal_measurement_is_subclass_of_measurement",
        1,
        QueryBehavioral,
        "conformance_temporal.rs::temporal_measurement_is_subclass_of_measurement",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_temporal_measurement_is_logic_relator",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saTemporalMeasurementIsLogicRelator",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_dating_method_is_subclass_of_observation_method",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saDatingMethodSubclassesObservationMethod",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measured_date_is_not_bridged_to_observation_result",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasuredDateNotBridgedToObservationResult",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measurement_method_bridges_to_observation_method",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasurementMethodBridgesToObservationMethod",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_measurement_determinacy_bridges_to_has_determinacy",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saMeasurementDeterminacyBridgesToHasDeterminacy",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_property_chain_period_start_from_measurement",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saPeriodStartPropertyChainFromMeasurement",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_measurement.py",
        "test_seed_measurements_carry_vantage_and_observed_feature",
        1,
        TBoxStructural,
        "slices/core/temporal/tests/structural.ttl#saSeedMeasurementsCarryVantageAndObservedFeature",
    ),
    // ── slices/core/temporal/tests/test_temporal.py (1) ─────────────────────────
    twin(
        "slices/core/temporal/tests/test_temporal.py",
        "test_reified_residence_and_tenure_are_time_scoped",
        1,
        QueryBehavioral,
        "conformance_temporal.rs::reified_residence_and_tenure_are_time_scoped",
    ),
    // ── slices/core/temporal/tests/test_temporal_query.py (9) ───────────────────
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_registry_covers_every_query_file",
        1,
        QueryBehavioral,
        "temporal.rs::registry_matches_the_python_surface",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_allen_closure_is_transitive",
        1,
        QueryBehavioral,
        "temporal.rs::allen_closure_is_transitive",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_before_event_reaches_lifeevents_and_orders_by_time",
        1,
        QueryBehavioral,
        "temporal.rs::before_event_reaches_lifeevents_and_orders_by_time",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_during_event_follows_relation_and_inverse",
        1,
        QueryBehavioral,
        "temporal.rs::during_event_follows_relation_and_inverse",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_timeline_orders_all_events_by_effective_start",
        1,
        QueryBehavioral,
        "temporal.rs::timeline_orders_all_events_by_effective_start",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_overlapping_window_matches_crisp_point_and_fuzzy",
        1,
        QueryBehavioral,
        "temporal.rs::overlapping_window_matches_crisp_point_and_fuzzy",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_bitemporal_four_clocks_returns_standpoint_indexed_claims",
        1,
        QueryBehavioral,
        "temporal.rs::bitemporal_four_clocks_returns_standpoint_indexed_claims",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_missing_parameter_is_rejected",
        1,
        QueryBehavioral,
        "temporal.rs::missing_parameter_is_a_hard_fail",
    ),
    twin(
        "slices/core/temporal/tests/test_temporal_query.py",
        "test_unknown_query_is_rejected",
        1,
        QueryBehavioral,
        "temporal.rs::unknown_query_name_is_rejected",
    ),
    // ── slices/extensions/music/tests/test_music_timbre.py (5) ──────────────────
    t(
        "slices/extensions/music/tests/test_music_timbre.py",
        "test_timbre_descriptor_seeds_exist",
    ),
    t(
        "slices/extensions/music/tests/test_music_timbre.py",
        "test_timbre_observation_result_property_exists",
    ),
    a(
        "slices/extensions/music/tests/test_music_timbre.py",
        "test_timbre_fixture_observations_exist",
    ),
    a(
        "slices/extensions/music/tests/test_music_timbre.py",
        "test_timbre_fixture_coequal_vantages",
    ),
    t(
        "slices/extensions/music/tests/test_music_timbre.py",
        "test_afo_timbre_mapping_exists",
    ),
    // ── tests/test_aboutness.py (2) — no slice dir → R5 ─────────────────────────
    q(
        "tests/test_aboutness.py",
        "test_aboutness_orthogonal_to_other_axes",
    ),
    q("tests/test_aboutness.py", "test_no_aboutness_truth_bridge"),
    // ── tests/test_aggregation.py (1) ───────────────────────────────────────────
    t(
        "tests/test_aggregation.py",
        "test_contains_place_exists_and_is_inverse",
    ),
    // ── tests/test_ai_claims.py (6) ─────────────────────────────────────────────
    q(
        "tests/test_ai_claims.py",
        "test_no_parallel_claim_construct_exists",
    ),
    q(
        "tests/test_ai_claims.py",
        "test_no_parallel_evaluation_construct_exists",
    ),
    q(
        "tests/test_ai_claims.py",
        "test_no_duplicate_provenance_properties",
    ),
    q(
        "tests/test_ai_claims.py",
        "test_no_winner_machinery_anywhere",
    ),
    q(
        "tests/test_ai_claims.py",
        "test_no_new_identity_axes_were_minted",
    ),
    q(
        "tests/test_ai_claims.py",
        "test_assessment_seam_is_the_norms_extensions",
    ),
    // ── tests/test_archaeological_evidence.py (2) ───────────────────────────────
    t(
        "tests/test_archaeological_evidence.py",
        "test_attested_on_carrier_exists",
    ),
    t(
        "tests/test_archaeological_evidence.py",
        "test_no_primary_or_preferred_archaeological_terms",
    ),
    // ── tests/test_cognition.py (4) ─────────────────────────────────────────────
    q(
        "tests/test_cognition.py",
        "test_mental_moment_has_exactly_one_gufo_metaclass",
    ),
    q(
        "tests/test_cognition.py",
        "test_cognition_sssom_rows_include_expected_alignments",
    ),
    q(
        "tests/test_cognition.py",
        "test_cognition_sssom_includes_corrected_wikidata_qids",
    ),
    q(
        "tests/test_cognition.py",
        "test_cognition_sssom_includes_opencyc_knows_about",
    ),
    // ── tests/test_contact_fields.py (3) ────────────────────────────────────────
    t("tests/test_contact_fields.py", "test_new_small_terms_exist"),
    t(
        "tests/test_contact_fields.py",
        "test_membership_relator_completed",
    ),
    t("tests/test_contact_fields.py", "test_no_flat_contact_terms"),
    // ── tests/test_coreference.py (1) ───────────────────────────────────────────
    t(
        "tests/test_coreference.py",
        "test_no_preferred_or_primary_coreference_terms",
    ),
    // ── tests/test_creative_works.py (1) ────────────────────────────────────────
    t(
        "tests/test_creative_works.py",
        "test_wemi_tiers_subclass_information_object",
    ),
    // ── tests/test_determinacy.py (1) — no slice dir → R5 ───────────────────────
    q(
        "tests/test_determinacy.py",
        "test_no_preferred_or_primary_term_is_declared",
    ),
    // ── tests/test_email_calendar.py (1) ────────────────────────────────────────
    a(
        "tests/test_email_calendar.py",
        "test_fixture_calendar_invitation_links_to_event",
    ),
    // ── tests/test_email_jmap.py (1) ────────────────────────────────────────────
    a(
        "tests/test_email_jmap.py",
        "test_fixture_includes_jmap_identifiers",
    ),
    // ── tests/test_email_mailbox.py (5) ─────────────────────────────────────────
    a(
        "tests/test_email_mailbox.py",
        "test_fixture_nested_hierarchy",
    ),
    a("tests/test_email_mailbox.py", "test_fixture_mailbox_paths"),
    a("tests/test_email_mailbox.py", "test_fixture_sort_orders"),
    a(
        "tests/test_email_mailbox.py",
        "test_fixture_destroyed_mailbox_uses_lifecycle",
    ),
    a(
        "tests/test_email_mailbox.py",
        "test_fixture_messages_in_nested_mailbox",
    ),
    // ── tests/test_email_thread_subject.py (1) ──────────────────────────────────
    a(
        "tests/test_email_thread_subject.py",
        "test_fixture_has_thread_subject_and_prefix",
    ),
    // ── tests/test_foundational_bridging.py (7) — no slice dir → R5 ─────────────
    q(
        "tests/test_foundational_bridging.py",
        "test_expected_cells_present_in_alignment_graph",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_bridge_uses_closematch_only",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_every_bfo_iri_is_a_real_class_in_the_snapshot",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_bridge_is_link_only_no_import",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_bfo_is_import_ok_upper_ontology",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_coverage_reported",
    ),
    q(
        "tests/test_foundational_bridging.py",
        "test_vendored_snapshot_matches_live_bfo",
    ),
    // ── tests/test_mereology.py (1) — no slice dir → R5 ─────────────────────────
    q(
        "tests/test_mereology.py",
        "test_no_winner_or_cardinality_terms_for_parts",
    ),
    // ── tests/test_narrative.py (4) ─────────────────────────────────────────────
    t(
        "tests/test_narrative.py",
        "test_narrative_reference_frame_is_not_standpoint_subclass",
    ),
    t(
        "tests/test_narrative.py",
        "test_book_release_and_serial_installment_are_creative_works",
    ),
    t(
        "tests/test_narrative.py",
        "test_frame_realm_narrative_and_frame_kind_narrative_exist",
    ),
    t(
        "tests/test_narrative.py",
        "test_reading_order_subclasses_standpoint",
    ),
    // ── tests/test_notation.py (2) ──────────────────────────────────────────────
    t(
        "tests/test_notation.py",
        "test_value_vocabularies_not_subclasses",
    ),
    t(
        "tests/test_notation.py",
        "test_ambiguous_cases_co_modelable",
    ),
    // ── tests/test_notes.py (8) — 4 traversal + 4 .rq parse guards ─────────────
    t(
        "tests/test_notes.py",
        "test_evidence_span_is_information_object",
    ),
    t(
        "tests/test_notes.py",
        "test_selector_sub_class_of_evidence_span",
    ),
    q(
        "tests/test_notes.py",
        "test_motivation_values_are_individuals",
    ),
    t("tests/test_notes.py", "test_notes_are_standpoint_indexed"),
    q("tests/test_notes.py", "test_notes_oa_projection_executable"),
    q(
        "tests/test_notes.py",
        "test_notes_schema_projection_executable",
    ),
    q("tests/test_notes.py", "test_notes_as_projection_executable"),
    q(
        "tests/test_notes.py",
        "test_notes_markdown_projection_executable",
    ),
    // ── tests/test_observations.py (1) ──────────────────────────────────────────
    q(
        "tests/test_observations.py",
        "test_kin_relationship_bridges_fire",
    ),
    // ── tests/test_privacy.py (2) — no slice dir; 1 mustNot + 1 projection ──────
    q(
        "tests/test_privacy.py",
        "test_no_preferred_or_primary_sensitivity_term",
    ),
    p(
        "tests/test_privacy.py",
        "test_odrl_projection_emits_privacy_policy",
    ),
    // ── tests/test_provenance.py (2) ────────────────────────────────────────────
    t(
        "tests/test_provenance.py",
        "test_carrier_and_ingestion_props",
    ),
    t(
        "tests/test_provenance.py",
        "test_four_clocks_are_distinct_dated_annotations",
    ),
    // ── tests/test_quality.py (1) ───────────────────────────────────────────────
    q(
        "tests/test_quality.py",
        "test_no_preferred_or_primary_term_is_declared",
    ),
    // ── tests/test_rights.py (7) — 1 count + 6 projection ──────────────────────
    q(
        "tests/test_rights.py",
        "test_expanded_action_vocabulary_is_seeded",
    ),
    p(
        "tests/test_rights.py",
        "test_odrl_projection_emits_a_policy_with_rules",
    ),
    p(
        "tests/test_rights.py",
        "test_odrl_projection_emits_constraint_and_conflict_logic",
    ),
    p(
        "tests/test_rights.py",
        "test_spdx_projection_emits_listed_license",
    ),
    p(
        "tests/test_rights.py",
        "test_cc_projection_emits_license_and_attribution",
    ),
    p(
        "tests/test_rights.py",
        "test_dcterms_projection_emits_flat_rights",
    ),
    p(
        "tests/test_rights.py",
        "test_schema_projection_emits_rights_cluster",
    ),
    // ── tests/test_rubrics.py (2) — no slice dir → R5 ──────────────────────────
    q(
        "tests/test_rubrics.py",
        "test_no_preferred_assessment_machinery",
    ),
    a(
        "tests/test_rubrics.py",
        "test_two_judges_disagree_without_contradiction",
    ),
    // ── tests/test_sensory_environment.py (4) ───────────────────────────────────
    t("tests/test_sensory_environment.py", "test_new_axes_exist"),
    t(
        "tests/test_sensory_environment.py",
        "test_perceptual_frame_realm_exists",
    ),
    q(
        "tests/test_sensory_environment.py",
        "test_sosa_alignments_loaded",
    ),
    q(
        "tests/test_sensory_environment.py",
        "test_psychological_mappings_loaded",
    ),
    // ── tests/test_suppression_conformance.py (3, parametrized) — R7 leak sweep ─
    pn(
        "tests/test_suppression_conformance.py",
        "test_suppressed_canary_never_leaks",
        27,
    ),
    pn(
        "tests/test_suppression_conformance.py",
        "test_precise_coarsened_values_never_leak",
        27,
    ),
    pn(
        "tests/test_suppression_conformance.py",
        "test_control_canary_proves_coverage",
        3,
    ),
    // ── tests/test_tags.py (1) ──────────────────────────────────────────────────
    t(
        "tests/test_tags.py",
        "test_no_bridge_among_has_tag_is_about_and_rdf_type",
    ),
    // ── tests/test_teleology.py (1) ─────────────────────────────────────────────
    t(
        "tests/test_teleology.py",
        "test_no_preferred_or_primary_goal_terms",
    ),
    // ── tests/test_trust.py (2) ─────────────────────────────────────────────────
    t(
        "tests/test_trust.py",
        "test_three_axes_are_orthogonal_in_trust",
    ),
    t(
        "tests/test_trust.py",
        "test_no_preferred_or_primary_trust_term",
    ),
    // ── tests/test_versions.py (1) ──────────────────────────────────────────────
    t(
        "tests/test_versions.py",
        "test_version_label_domain_is_entity",
    ),
];

/// Every row whose `python_file` equals `file`.
pub fn rows_for(file: &str) -> Vec<&'static ManifestRow> {
    MANIFEST.iter().filter(|r| r.python_file == file).collect()
}

/// The 35 in-scope files with their pinned `(def test_ count, dropped count)` —
/// the git-anchored inventory captured at the merge-base before deletion. `total`
/// is the `grep -c 'def test_'` fn count of the source file (NOT the logical-case
/// count); the parametrize cardinality lives in each row's `case_count` and is
/// checked separately by [`parametrized_rows_declare_cardinality`].
const EXPECTED: &[(&str, usize, usize)] = &[
    ("slices/core/temporal/tests/test_temporal_frame.py", 7, 0),
    (
        "slices/core/temporal/tests/test_temporal_measurement.py",
        13,
        0,
    ),
    ("slices/core/temporal/tests/test_temporal.py", 1, 0),
    ("slices/core/temporal/tests/test_temporal_query.py", 9, 0),
    ("slices/extensions/music/tests/test_music_timbre.py", 5, 0),
    ("tests/test_aboutness.py", 2, 0),
    ("tests/test_aggregation.py", 1, 0),
    ("tests/test_ai_claims.py", 6, 0),
    ("tests/test_archaeological_evidence.py", 2, 0),
    ("tests/test_cognition.py", 4, 0),
    ("tests/test_contact_fields.py", 3, 0),
    ("tests/test_coreference.py", 1, 0),
    ("tests/test_creative_works.py", 1, 0),
    ("tests/test_determinacy.py", 1, 0),
    ("tests/test_email_calendar.py", 1, 0),
    ("tests/test_email_jmap.py", 1, 0),
    ("tests/test_email_mailbox.py", 5, 0),
    ("tests/test_email_thread_subject.py", 1, 0),
    ("tests/test_foundational_bridging.py", 7, 0),
    ("tests/test_mereology.py", 1, 0),
    ("tests/test_narrative.py", 4, 0),
    ("tests/test_notation.py", 2, 0),
    ("tests/test_notes.py", 8, 0),
    ("tests/test_observations.py", 1, 0),
    ("tests/test_privacy.py", 2, 0),
    ("tests/test_provenance.py", 2, 0),
    ("tests/test_quality.py", 1, 0),
    ("tests/test_rights.py", 7, 0),
    ("tests/test_rubrics.py", 2, 0),
    ("tests/test_sensory_environment.py", 4, 0),
    ("tests/test_suppression_conformance.py", 3, 0),
    ("tests/test_tags.py", 1, 0),
    ("tests/test_teleology.py", 1, 0),
    ("tests/test_trust.py", 2, 0),
    ("tests/test_versions.py", 1, 0),
];

#[test]
fn manifest_rows_are_nonempty_and_deduped() {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for row in MANIFEST {
        assert!(
            !row.python_file.is_empty() && !row.python_fn.is_empty(),
            "manifest row has an empty file or fn"
        );
        assert!(
            row.case_count >= 1,
            "case_count must be >= 1 for {}::{}",
            row.python_file,
            row.python_fn
        );
        let key = (row.python_file, row.python_fn);
        assert!(
            !seen.contains(&key),
            "duplicate manifest row {}::{}",
            row.python_file,
            row.python_fn
        );
        seen.push(key);
    }
}

#[test]
fn twin_paths_are_well_formed() {
    // Any row that ALREADY has a twin carries a non-empty locator whose shape
    // matches its obligation kind. `Pending`/`Dropped` rows are not checked here —
    // this passes mid-branch while later tasks fill twins.
    for row in MANIFEST {
        if let TwinState::Twin(locator) = row.state {
            assert!(
                !locator.is_empty(),
                "empty twin locator for {}::{}",
                row.python_file,
                row.python_fn
            );
        }
    }
}

#[test]
fn twin_kind_matches_locator_surface() {
    // The typed coverage proof: a twin must occupy the surface its obligation kind
    // demands. A `TBoxStructural`/`AboxWitness` twin is a slice cell IRI (the
    // dogfooded surface); a behavioral/projection twin is a Rust `file.rs::fn`. This
    // stops a twin from claiming a dogfooded home it does not actually occupy.
    for row in MANIFEST {
        if let TwinState::Twin(locator) = row.state {
            match row.kind {
                TBoxStructural => assert!(
                    locator.contains("/tests/structural.ttl#"),
                    "TBoxStructural twin {locator:?} for {}::{} must be a structural.ttl cell IRI",
                    row.python_file,
                    row.python_fn
                ),
                AboxWitness => assert!(
                    locator.contains("/tests/example-conformance.ttl#"),
                    "AboxWitness twin {locator:?} for {}::{} must be an example-conformance.ttl cell IRI",
                    row.python_file,
                    row.python_fn
                ),
                QueryBehavioral | ProjectionPreservation => assert!(
                    locator.contains(".rs::"),
                    "behavioral/projection twin {locator:?} for {}::{} must be a `file.rs::fn` locator",
                    row.python_file,
                    row.python_fn
                ),
            }
        }
    }
}

#[test]
fn parametrized_rows_declare_cardinality() {
    // The only parametrized source file is test_suppression_conformance.py; its three
    // fns parametrize over the projection registry. Pin the cardinalities so a twin
    // cannot silently narrow a 27-profile sweep to one profile.
    let supp = rows_for("tests/test_suppression_conformance.py");
    assert_eq!(supp.len(), 3, "expected 3 suppression rows");
    for row in &supp {
        let expected = match row.python_fn {
            "test_suppressed_canary_never_leaks" => 27,
            "test_precise_coarsened_values_never_leak" => 27,
            "test_control_canary_proves_coverage" => 3,
            other => panic!("unexpected suppression fn {other}"),
        };
        assert_eq!(
            row.case_count, expected,
            "{}::{} declares case_count {}, expected {expected}",
            row.python_file, row.python_fn, row.case_count
        );
    }
}

#[test]
#[ignore = "armed at migration finalization (Task 7): un-ignore once every row is a Twin. \
            Kept in-tree from scaffold so the completeness gate is visible (shows as `ignored`), \
            not forgotten, while mid-branch commits stay green."]
fn migration_is_reconciled_and_complete() {
    // Finalization gate: every source fn is a native `Twin` (target: zero `Dropped`),
    // no `Pending` survives, and each file's fn count matches the git-anchored
    // inventory. A future edit that drops or double-counts a twin trips this.
    for row in MANIFEST {
        assert!(
            row.state != TwinState::Pending,
            "unreconciled Pending row {}::{} — every source fn must be a Twin or a Drop",
            row.python_file,
            row.python_fn
        );
    }
    for (file, total, dropped) in EXPECTED {
        let rows = rows_for(file);
        assert_eq!(
            rows.len(),
            *total,
            "manifest has {} rows for {file}, expected {total}",
            rows.len()
        );
        let drops = rows
            .iter()
            .filter(|r| r.state == TwinState::Dropped)
            .count();
        let twins = rows
            .iter()
            .filter(|r| matches!(r.state, TwinState::Twin(_)))
            .count();
        assert_eq!(
            drops, *dropped,
            "{file}: {drops} Dropped rows, expected {dropped}"
        );
        assert_eq!(
            twins,
            *total - *dropped,
            "{file}: {twins} Twin rows, expected {}",
            *total - *dropped
        );
    }
    let mut files: Vec<&str> = MANIFEST.iter().map(|r| r.python_file).collect();
    files.sort_unstable();
    files.dedup();
    assert_eq!(
        files.len(),
        EXPECTED.len(),
        "manifest covers {} files, expected {}",
        files.len(),
        EXPECTED.len()
    );
}

#[test]
fn obligation_kind_tally() {
    // Utility: print the obligation-kind split so the PR shows mechanically how much
    // of the cluster moved to the dogfooded slice cells vs. escaped to Rust. Not a
    // gate — a visibility surface. (Runs even while rows are Pending, using the
    // provisional kinds.)
    let (mut tbox, mut abox, mut query, mut proj, mut cases) = (0usize, 0, 0, 0, 0usize);
    for row in MANIFEST {
        cases += row.case_count;
        match row.kind {
            TBoxStructural => tbox += 1,
            AboxWitness => abox += 1,
            QueryBehavioral => query += 1,
            ProjectionPreservation => proj += 1,
        }
    }
    println!(
        "graph-migration obligation tally: {} fns / {} logical cases — TBoxStructural={tbox} AboxWitness={abox} QueryBehavioral={query} ProjectionPreservation={proj}",
        MANIFEST.len(),
        cases
    );
    assert_eq!(
        MANIFEST.len(),
        113,
        "expected 113 source fns in the graph-traversal cluster"
    );
}
