// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Single-process owner for the complete whole-ontology conformance inventory.
//!
//! Each case module registers its named contracts at link time. One required libtest drives
//! all per-commit contracts, and one maint-heavy sibling drives the exhaustive inventory.
//! Each lane therefore shares one exact authenticated bundle, parsed SHACL model, and set of
//! derived read indexes without dropping any assertion.

mod conformance_support;

#[path = "conformance_cases/conformance_aboutness.rs"]
mod conformance_aboutness;
#[path = "conformance_cases/conformance_affect.rs"]
mod conformance_affect;
#[path = "conformance_cases/conformance_affect_producer.rs"]
mod conformance_affect_producer;
#[path = "conformance_cases/conformance_affect_producer_union.rs"]
mod conformance_affect_producer_union;
#[path = "conformance_cases/conformance_agentic.rs"]
mod conformance_agentic;
#[path = "conformance_cases/conformance_aggregation.rs"]
mod conformance_aggregation;
#[path = "conformance_cases/conformance_ai_claims.rs"]
mod conformance_ai_claims;
#[path = "conformance_cases/conformance_ai_claims_tbox.rs"]
mod conformance_ai_claims_tbox;
#[path = "conformance_cases/conformance_allen_jepd.rs"]
mod conformance_allen_jepd;
#[path = "conformance_cases/conformance_archaeological_evidence.rs"]
mod conformance_archaeological_evidence;
#[path = "conformance_cases/conformance_awareness.rs"]
mod conformance_awareness;
#[path = "conformance_cases/conformance_calendar.rs"]
mod conformance_calendar;
#[path = "conformance_cases/conformance_citations.rs"]
mod conformance_citations;
#[path = "conformance_cases/conformance_cognition.rs"]
mod conformance_cognition;
#[path = "conformance_cases/conformance_competency.rs"]
mod conformance_competency;
#[path = "conformance_cases/conformance_contacts.rs"]
mod conformance_contacts;
#[path = "conformance_cases/conformance_coreference.rs"]
mod conformance_coreference;
#[path = "conformance_cases/conformance_creative_works.rs"]
mod conformance_creative_works;
#[path = "conformance_cases/conformance_deception.rs"]
mod conformance_deception;
#[path = "conformance_cases/conformance_determinacy.rs"]
mod conformance_determinacy;
#[path = "conformance_cases/conformance_disclosure.rs"]
mod conformance_disclosure;
#[path = "conformance_cases/conformance_dreaming.rs"]
mod conformance_dreaming;
#[path = "conformance_cases/conformance_email.rs"]
mod conformance_email;
#[path = "conformance_cases/conformance_embedding_projection.rs"]
mod conformance_embedding_projection;
#[path = "conformance_cases/conformance_employment.rs"]
mod conformance_employment;
#[path = "conformance_cases/conformance_epistemics.rs"]
mod conformance_epistemics;
#[path = "conformance_cases/conformance_events.rs"]
mod conformance_events;
#[path = "conformance_cases/conformance_evidence.rs"]
mod conformance_evidence;
#[path = "conformance_cases/conformance_expertise.rs"]
mod conformance_expertise;
#[path = "conformance_cases/conformance_finance.rs"]
mod conformance_finance;
#[path = "conformance_cases/conformance_foundational_bridging.rs"]
mod conformance_foundational_bridging;
#[path = "conformance_cases/conformance_gender.rs"]
mod conformance_gender;
#[path = "conformance_cases/conformance_genealogy.rs"]
mod conformance_genealogy;
#[path = "conformance_cases/conformance_gts_slice.rs"]
mod conformance_gts_slice;
#[path = "conformance_cases/conformance_identity_orthogonality.rs"]
mod conformance_identity_orthogonality;
#[path = "conformance_cases/conformance_identity_over_history.rs"]
mod conformance_identity_over_history;
#[path = "conformance_cases/conformance_images.rs"]
mod conformance_images;
#[path = "conformance_cases/conformance_imagination.rs"]
mod conformance_imagination;
#[path = "conformance_cases/conformance_inference.rs"]
mod conformance_inference;
#[path = "conformance_cases/conformance_interior.rs"]
mod conformance_interior;
#[path = "conformance_cases/conformance_lifecycle.rs"]
mod conformance_lifecycle;
#[path = "conformance_cases/conformance_math_grounding.rs"]
mod conformance_math_grounding;
#[path = "conformance_cases/conformance_math_norm.rs"]
mod conformance_math_norm;
#[path = "conformance_cases/conformance_math_producers.rs"]
mod conformance_math_producers;
#[path = "conformance_cases/conformance_mereology.rs"]
mod conformance_mereology;
#[path = "conformance_cases/conformance_music.rs"]
mod conformance_music;
#[path = "conformance_cases/conformance_music_analysis.rs"]
mod conformance_music_analysis;
#[path = "conformance_cases/conformance_music_collections.rs"]
mod conformance_music_collections;
#[path = "conformance_cases/conformance_music_competency.rs"]
mod conformance_music_competency;
#[path = "conformance_cases/conformance_music_instruments.rs"]
mod conformance_music_instruments;
#[path = "conformance_cases/conformance_music_oral_tradition.rs"]
mod conformance_music_oral_tradition;
#[path = "conformance_cases/conformance_music_performance.rs"]
mod conformance_music_performance;
#[path = "conformance_cases/conformance_music_performance_events.rs"]
mod conformance_music_performance_events;
#[path = "conformance_cases/conformance_music_pitch.rs"]
mod conformance_music_pitch;
#[path = "conformance_cases/conformance_music_structure.rs"]
mod conformance_music_structure;
#[path = "conformance_cases/conformance_music_time.rs"]
mod conformance_music_time;
#[path = "conformance_cases/conformance_myth.rs"]
mod conformance_myth;
#[path = "conformance_cases/conformance_names.rs"]
mod conformance_names;
#[path = "conformance_cases/conformance_narration.rs"]
mod conformance_narration;
#[path = "conformance_cases/conformance_narrative.rs"]
mod conformance_narrative;
#[path = "conformance_cases/conformance_narrative_time.rs"]
mod conformance_narrative_time;
#[path = "conformance_cases/conformance_norms.rs"]
mod conformance_norms;
#[path = "conformance_cases/conformance_notation.rs"]
mod conformance_notation;
#[path = "conformance_cases/conformance_notes.rs"]
mod conformance_notes;
#[path = "conformance_cases/conformance_observations.rs"]
mod conformance_observations;
#[path = "conformance_cases/conformance_organization.rs"]
mod conformance_organization;
#[path = "conformance_cases/conformance_places.rs"]
mod conformance_places;
#[path = "conformance_cases/conformance_privacy.rs"]
mod conformance_privacy;
#[path = "conformance_cases/conformance_profiles.rs"]
mod conformance_profiles;
#[path = "conformance_cases/conformance_provenance.rs"]
mod conformance_provenance;
#[path = "conformance_cases/conformance_quality.rs"]
mod conformance_quality;
#[path = "conformance_cases/conformance_reference_frames.rs"]
mod conformance_reference_frames;
#[path = "conformance_cases/conformance_registers.rs"]
mod conformance_registers;
#[path = "conformance_cases/conformance_rights.rs"]
mod conformance_rights;
#[path = "conformance_cases/conformance_risk.rs"]
mod conformance_risk;
#[path = "conformance_cases/conformance_rubrics.rs"]
mod conformance_rubrics;
#[path = "conformance_cases/conformance_sensory.rs"]
mod conformance_sensory;
#[path = "conformance_cases/conformance_sensory_environment.rs"]
mod conformance_sensory_environment;
#[path = "conformance_cases/conformance_sexuality.rs"]
mod conformance_sexuality;
#[path = "conformance_cases/conformance_software.rs"]
mod conformance_software;
#[path = "conformance_cases/conformance_sparql_features.rs"]
mod conformance_sparql_features;
#[path = "conformance_cases/conformance_sparql_surface.rs"]
mod conformance_sparql_surface;
#[path = "conformance_cases/conformance_standpoint.rs"]
mod conformance_standpoint;
#[path = "conformance_cases/conformance_support_tests.rs"]
mod conformance_support_tests;
#[path = "conformance_cases/conformance_suppression.rs"]
mod conformance_suppression;
#[path = "conformance_cases/conformance_tags.rs"]
mod conformance_tags;
#[path = "conformance_cases/conformance_teleology.rs"]
mod conformance_teleology;
#[path = "conformance_cases/conformance_temporal.rs"]
mod conformance_temporal;
#[path = "conformance_cases/conformance_trust.rs"]
mod conformance_trust;
#[path = "conformance_cases/conformance_verifiable_release_chain.rs"]
mod conformance_verifiable_release_chain;
#[path = "conformance_cases/conformance_versions.rs"]
mod conformance_versions;
#[path = "conformance_cases/conformance_vocabulary_surface.rs"]
mod conformance_vocabulary_surface;
#[path = "conformance_cases/ontology_conformance.rs"]
mod ontology_conformance;

#[test]
fn required_conformance_contracts_share_one_authenticated_corpus() {
    conformance_support::run_registered_conformance_contracts(
        conformance_support::ConformanceLane::Required,
    );
}

#[test]
fn maint_heavy_conformance_contracts_share_one_authenticated_corpus_heavy_offgate() {
    conformance_support::run_registered_conformance_contracts(
        conformance_support::ConformanceLane::MaintHeavy,
    );
}
