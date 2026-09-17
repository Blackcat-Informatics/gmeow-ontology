// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated native conformance consumers. Authored corpus execution belongs
//! to the optimized producer; these gates preserve the original gold assertions.

mod advice_wing_fixture;
mod cl_ingest;
mod common;
mod contextual_common;
mod contextual_corpus;
mod diagnostics_gate_morphism;
mod diagnostics_meta_findings;
mod dl_oracle_gold;
mod documentation_graph;
mod el_divergence_gate;
mod entailment_mini_gate;
mod epistemics;
mod expression_substrate;
mod flagship_shape_unwired;
mod full_divergence_gate;
mod full_native_gate;
mod glossary;
mod gmn_consume;
mod gmn_cost_feed;
mod gmn_dictionary;
mod gmn_grounding;
mod gmn_logic_signature_coherence;
mod gmn_math_signature_coherence;
mod gmn_migration;
mod gmn_pack;
mod gmn_signatures;
mod gmn_vectors;
mod gufo_superset;
mod language_catalog;
mod logic_module_contracts;
mod math_lowering;
mod native_fragment_coverage_gate;
mod native_scene;
mod numeric_builtin_oracle_gold;
mod ontouml_divergence_gate;
mod refutation;
mod source_artifact;
mod temporal_corpus;
mod tptp_divergence_gate;
mod tptp_proof;
mod verify_gates;
mod wellfounded_plan_parity;
