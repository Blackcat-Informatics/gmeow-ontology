// SPDX-License-Identifier: AGPL-3.0-only

//! The Python-fn → native-Rust-twin reconciliation manifest for the SPARQL
//! conformance migration.
//!
//! One [`ManifestRow`] per Python test fn across the 16 files in scope. The LEFT
//! side (`python_file`, `python_fn`) is the authoritative inventory, sourced now:
//! - the 12 dossiered files from their `docs/test-retention/test_*.md` "Retained
//!   dynamic tests" lists (each retained count equals the `grep -c 'def test_'`
//!   count of the source file — verified), and
//! - the 4 undocumented slice files from `grep 'def test_'` on the Python source.
//!
//! The RIGHT side is the reconciliation state ([`TwinState`]): each cluster task
//! FILLS a [`TwinState::Pending`] row by editing it to [`TwinState::Twin`] with the
//! twin's `file.rs::fn`-style locator. [`TwinState::Dropped`] rows are behaviour
//! intentionally retired (never to get a twin).
//!
//! The [`twin_paths_are_well_formed`] test only checks that any row that ALREADY
//! has a twin carries a non-empty, well-formed locator, so it passes mid-branch
//! while `Pending` rows remain. Task 7 finalizes the completeness gate ("every
//! non-`Dropped` row has a `Twin`, and per-file twin counts match the dossier").

/// The reconciliation state of a Python test fn during the native migration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TwinState {
    /// A native Rust twin exists, at this `file.rs::fn`-style locator.
    Twin(&'static str),
    /// No twin yet — a later cluster task fills this slot.
    Pending,
    /// Behaviour intentionally retired (not ported), never to get a twin.
    Dropped,
}

/// One `(python_file, python_fn) → reconciliation state` row.
#[derive(Clone, Copy, Debug)]
pub struct ManifestRow {
    /// Repo-relative path of the Python test file.
    pub python_file: &'static str,
    /// The Python test fn name (`test_*`).
    pub python_fn: &'static str,
    /// Reconciliation state (later tasks edit `Pending` → `Twin(..)`).
    pub state: TwinState,
}

/// A row whose twin a later task must supply.
const fn pending(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        state: TwinState::Pending,
    }
}

/// A row whose behaviour is intentionally dropped (no twin).
const fn dropped(python_file: &'static str, python_fn: &'static str) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        state: TwinState::Dropped,
    }
}

/// A row with a landed native twin at `locator` (`file.rs::fn`). Later tasks call
/// this in place of [`pending`] when they land a twin.
#[allow(dead_code)]
const fn twin(
    python_file: &'static str,
    python_fn: &'static str,
    locator: &'static str,
) -> ManifestRow {
    ManifestRow {
        python_file,
        python_fn,
        state: TwinState::Twin(locator),
    }
}

/// The full reconciliation manifest — 81 rows across the 16 in-scope files.
pub const MANIFEST: &[ManifestRow] = &[
    // ── tests/test_compat_rdflib.py (26 fns; dossier test_compat_rdflib.md) ────
    // Per user ruling only the 2 SPARQL fns get twins; the other 24 are Dropped.
    dropped(
        "tests/test_compat_rdflib.py",
        "test_submodule_import_after_shim_swap",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_terms_are_str_subclasses",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_literal_value_and_topython",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_literal_term_equality_xsd_string_asymmetry",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_literal_rewrap_preserves_integer_subtype",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_literal_value_space_eq_is_separate_from_term_equality",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_literal_ordering_uses_value_then_term_fallback",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_graph_add_value_contains_and_xsd_string_provenance",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_graph_keeps_plain_and_explicit_xsd_string_as_separate_terms",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_parsed_graph_string_patterns_use_native_value_space",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_graph_numeric_literal_contains_uses_value_space",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_graph_accessors_and_wildcards",
    ),
    dropped("tests/test_compat_rdflib.py", "test_remove_and_set"),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_graph_intersection_symmetric_difference_and_update",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_dataset_named_graph_quads_filtering",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_turtle_roundtrip_and_isomorphic",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_private_language_tag_survives_cow_materialization",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_serialize_nt_encoding_contract",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_collection_write_read_roundtrip",
    ),
    // The 2 SPARQL fns that DO get twins (native surface twins):
    twin(
        "tests/test_compat_rdflib.py",
        "test_sparql_select_ask_construct_and_resultrow",
        "conformance_sparql_surface.rs::select_ask_construct_surface",
    ),
    twin(
        "tests/test_compat_rdflib.py",
        "test_query_initbindings_nonprojected_var",
        "conformance_sparql_surface.rs::initbindings_binds_nonprojected_var",
    ),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_to_canonical_graph_and_graph_diff",
    ),
    dropped("tests/test_compat_rdflib.py", "test_guess_format"),
    dropped("tests/test_compat_rdflib.py", "test_jsonld_star_roundtrip"),
    dropped("tests/test_compat_rdflib.py", "test_rdfxml_roundtrip"),
    dropped(
        "tests/test_compat_rdflib.py",
        "test_namespace_attribute_and_item_access",
    ),
    // ── tests/test_interior.py (8 fns; dossier test_interior.md) ───────────────
    twin(
        "tests/test_interior.py",
        "test_plutchik_seeds_are_present_and_open",
        "conformance_interior.rs::plutchik_seeds_are_present_and_open",
    ),
    twin(
        "tests/test_interior.py",
        "test_appraisal_is_a_vantage_indexed_observation",
        "conformance_interior.rs::appraisal_is_a_vantage_indexed_observation",
    ),
    twin(
        "tests/test_interior.py",
        "test_no_emotion_tenure_class_exists",
        "conformance_interior.rs::no_emotion_tenure_class_exists",
    ),
    twin(
        "tests/test_interior.py",
        "test_arc_sample_constituents",
        "conformance_interior.rs::arc_sample_constituents",
    ),
    twin(
        "tests/test_interior.py",
        "test_character_arc_extension_is_additive",
        "conformance_interior.rs::character_arc_extension_is_additive",
    ),
    twin(
        "tests/test_interior.py",
        "test_no_primary_protagonist_machinery",
        "conformance_interior.rs::no_primary_protagonist_machinery",
    ),
    twin(
        "tests/test_interior.py",
        "test_motif_rides_the_seam",
        "conformance_interior.rs::motif_rides_the_seam",
    ),
    twin(
        "tests/test_interior.py",
        "test_trajectory_query_orders_and_surfaces_disagreement",
        "conformance_interior.rs::trajectory_query_orders_and_surfaces_disagreement",
    ),
    // ── tests/test_narration.py (6 fns; dossier test_narration.md) ─────────────
    twin(
        "tests/test_narration.py",
        "test_seam_links_specialize_one_ancestor",
        "conformance_narration.rs::seam_links_specialize_one_ancestor",
    ),
    twin(
        "tests/test_narration.py",
        "test_orientations_are_not_inverse_axioms",
        "conformance_narration.rs::orientations_are_not_inverse_axioms",
    ),
    twin(
        "tests/test_narration.py",
        "test_narration_mode_vocab_seeds",
        "conformance_narration.rs::narration_mode_vocab_seeds",
    ),
    twin(
        "tests/test_narration.py",
        "test_no_truth_bridge_from_unreliable_mode",
        "conformance_narration.rs::no_truth_bridge_from_unreliable_mode",
    ),
    twin(
        "tests/test_narration.py",
        "test_fixture_obeys_the_efficiency_budget",
        "conformance_narration.rs::fixture_obeys_the_efficiency_budget",
    ),
    twin(
        "tests/test_narration.py",
        "test_competency_cooccurrence_query_over_fixture",
        "conformance_narration.rs::competency_cooccurrence_query_over_fixture",
    ),
    // ── tests/test_narrative_time.py (4 fns; dossier test_narrative_time.md) ────
    twin(
        "tests/test_narrative_time.py",
        "test_frame_properties_are_functional_with_correct_anchors",
        "conformance_narrative_time.rs::frame_properties_are_functional_with_correct_anchors",
    ),
    twin(
        "tests/test_narrative_time.py",
        "test_at_narrative_position_is_domain_free_and_not_functional",
        "conformance_narrative_time.rs::at_narrative_position_is_domain_free_and_not_functional",
    ),
    twin(
        "tests/test_narrative_time.py",
        "test_flashback_fixture_carries_coexisting_orders",
        "conformance_narrative_time.rs::flashback_fixture_carries_coexisting_orders",
    ),
    twin(
        "tests/test_narrative_time.py",
        "test_competency_narrative_time_axes_query",
        "conformance_narrative_time.rs::competency_narrative_time_axes_query",
    ),
    // ── tests/test_disclosure.py (4 fns; dossier test_disclosure.md) ───────────
    twin(
        "tests/test_disclosure.py",
        "test_no_preferred_or_primary_disclosure_term",
        "conformance_disclosure.rs::no_preferred_or_primary_disclosure_term",
    ),
    twin(
        "tests/test_disclosure.py",
        "test_project_when_in_sparql_query",
        "conformance_disclosure.rs::project_when_gates_description_on_public_eligibility",
    ),
    twin(
        "tests/test_disclosure.py",
        "test_public_candidates_query_runnable",
        "conformance_disclosure.rs::public_candidates_query_runnable",
    ),
    twin(
        "tests/test_disclosure.py",
        "test_privacy_leaks_query_runnable",
        "conformance_disclosure.rs::privacy_leaks_query_runnable",
    ),
    // ── tests/test_email_behavioral.py (3 fns; dossier) ────────────────────────
    twin(
        "tests/test_email_behavioral.py",
        "test_fixture_dsn_has_overlapping_kinds",
        "conformance_email.rs::dsn_has_overlapping_kinds",
    ),
    twin(
        "tests/test_email_behavioral.py",
        "test_fixture_auto_generated_message",
        "conformance_email.rs::auto_generated_message",
    ),
    twin(
        "tests/test_email_behavioral.py",
        "test_fixture_read_receipt_request",
        "conformance_email.rs::read_receipt_request",
    ),
    // ── tests/test_email_participant.py (3 fns; dossier) ───────────────────────
    twin(
        "tests/test_email_participant.py",
        "test_resent_properties_are_multivalued_in_linkml_schema",
        "conformance_email.rs::resent_properties_are_multivalued_in_linkml_schema",
    ),
    twin(
        "tests/test_email_participant.py",
        "test_fixture_binds_occurrence_correctly",
        "conformance_email.rs::binds_occurrence_correctly",
    ),
    twin(
        "tests/test_email_participant.py",
        "test_fixture_address_decomposition",
        "conformance_email.rs::address_decomposition",
    ),
    // ── tests/test_email_versions.py (3 fns; dossier) ──────────────────────────
    twin(
        "tests/test_email_versions.py",
        "test_fixture_version_memberships_use_roles_not_subclasses",
        "conformance_email.rs::version_memberships_use_roles_not_subclasses",
    ),
    twin(
        "tests/test_email_versions.py",
        "test_fixture_patch_diff_links_and_digest",
        "conformance_email.rs::patch_diff_links_and_digest",
    ),
    twin(
        "tests/test_email_versions.py",
        "test_fixture_collision_flags_and_fingerprints",
        "conformance_email.rs::collision_flags_and_fingerprints",
    ),
    // ── tests/test_gender.py (2 fns; dossier test_gender.md) ───────────────────
    twin(
        "tests/test_gender.py",
        "test_displayable_generalised_to_cover_identity",
        "conformance_gender.rs::displayable_generalised_to_cover_identity",
    ),
    twin(
        "tests/test_gender.py",
        "test_competency_gender_values_query",
        "conformance_gender.rs::competency_gender_values_query",
    ),
    // ── tests/test_sexuality.py (1 fn; dossier test_sexuality.md) ──────────────
    twin(
        "tests/test_sexuality.py",
        "test_competency_orientation_values_query",
        "conformance_sexuality.rs::competency_orientation_values_query",
    ),
    // ── tests/test_risk.py (2 fns; dossier test_risk.md) ───────────────────────
    twin(
        "tests/test_risk.py",
        "test_no_occurrence_gate",
        "conformance_risk.rs::no_occurrence_gate",
    ),
    twin(
        "tests/test_risk.py",
        "test_competency_severity_order_query",
        "conformance_risk.rs::competency_severity_order_query",
    ),
    // ── tests/test_competency.py (1 fn; dossier test_competency.md) ────────────
    twin(
        "tests/test_competency.py",
        "test_competency_expertise_expiring_credentials_query",
        "conformance_competency.rs::competency_expertise_expiring_credentials_query",
    ),
    // ── slices/core/gts/tests/test_gts_slice.py (2 fns; no dossier, from source)
    twin(
        "slices/core/gts/tests/test_gts_slice.py",
        "test_value_vocabulary_cardinality_floors",
        "conformance_gts_slice.rs::value_vocabulary_cardinality_floors",
    ),
    twin(
        "slices/core/gts/tests/test_gts_slice.py",
        "test_competency_queries_parse_and_run",
        "conformance_gts_slice.rs::competency_queries_parse_and_run",
    ),
    // ── slices/extensions/dreaming/tests/test_dreaming.py (5 fns; from source) ──
    twin(
        "slices/extensions/dreaming/tests/test_dreaming.py",
        "test_dream_experience_composition",
        "conformance_dreaming.rs::dream_experience_composition",
    ),
    twin(
        "slices/extensions/dreaming/tests/test_dreaming.py",
        "test_dream_report_composition",
        "conformance_dreaming.rs::dream_report_composition",
    ),
    twin(
        "slices/extensions/dreaming/tests/test_dreaming.py",
        "test_dream_element_links",
        "conformance_dreaming.rs::dream_element_links",
    ),
    twin(
        "slices/extensions/dreaming/tests/test_dreaming.py",
        "test_lucid_dream_uses_mode_lucid_dreaming",
        "conformance_dreaming.rs::lucid_dream_uses_mode_lucid_dreaming",
    ),
    twin(
        "slices/extensions/dreaming/tests/test_dreaming.py",
        "test_memory_consolidation_replay",
        "conformance_dreaming.rs::memory_consolidation_replay",
    ),
    // ── slices/extensions/music/tests/test_music_competency.py (1 fn; source) ──
    twin(
        "slices/extensions/music/tests/test_music_competency.py",
        "test_music_competency_query",
        "conformance_music_competency.rs::music_competency_query",
    ),
    // ── slices/extensions/music/tests/test_music_oral_tradition.py (10; source) ─
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_oral_tradition_work_fixture_exists",
        "conformance_music_oral_tradition.rs::oral_tradition_work_fixture_exists",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_oral_tradition_expressions_have_no_notated_member",
        "conformance_music_oral_tradition.rs::oral_tradition_expressions_have_no_notated_member",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_performance_lineage_derivation_chain",
        "conformance_music_oral_tradition.rs::performance_lineage_derivation_chain",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_tune_family_is_versionset",
        "conformance_music_oral_tradition.rs::tune_family_is_versionset",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_versionset_reused_unchanged",
        "conformance_music_oral_tradition.rs::versionset_reused_unchanged",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_contested_membership_is_suppressed_not_deleted",
        "conformance_music_oral_tradition.rs::contested_membership_is_suppressed_not_deleted",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_transmission_event_and_roles",
        "conformance_music_oral_tradition.rs::transmission_event_and_roles",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_no_shape_requires_notated_expression",
        "conformance_music_oral_tradition.rs::no_shape_requires_notated_expression",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_competency_query_oral_works",
        "conformance_music_oral_tradition.rs::competency_query_oral_works",
    ),
    twin(
        "slices/extensions/music/tests/test_music_oral_tradition.py",
        "test_competency_query_gharana_memberships",
        "conformance_music_oral_tradition.rs::competency_query_gharana_memberships",
    ),
];

/// Every row whose `python_file` equals `file` — the accessor Task 7 uses for its
/// per-file count gate.
pub fn rows_for(file: &str) -> Vec<&'static ManifestRow> {
    MANIFEST.iter().filter(|r| r.python_file == file).collect()
}

#[test]
fn twin_paths_are_well_formed() {
    // Any row that ALREADY has a twin must carry a non-empty, `file::fn`-style
    // locator. `Pending`/`Dropped` rows are not checked here — this test passes
    // mid-branch while later tasks fill twins.
    //
    // Task 7 finalizes the completeness gate (every non-`Dropped` row is a `Twin`,
    // and each file's twin count matches its dossier's "Retained dynamic tests").
    for row in MANIFEST {
        if let TwinState::Twin(locator) = row.state {
            assert!(
                !locator.is_empty(),
                "empty twin locator for {}::{}",
                row.python_file,
                row.python_fn
            );
            assert!(
                locator.contains("::"),
                "twin locator {locator:?} for {}::{} must be `file::fn`-style",
                row.python_file,
                row.python_fn
            );
        }
    }
}

#[test]
fn manifest_rows_are_nonempty_and_deduped() {
    // The LEFT-side inventory is load-bearing: no blank cells, no duplicate
    // (file, fn) pairs.
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for row in MANIFEST {
        assert!(
            !row.python_file.is_empty() && !row.python_fn.is_empty(),
            "manifest row has an empty file or fn"
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
