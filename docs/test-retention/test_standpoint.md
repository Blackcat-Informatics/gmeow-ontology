# Retention: `tests/test_standpoint.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The standpoint / contested-claims facility (#43).

Retained dynamic tests:

- `test_three_axes_are_orthogonal` — standpoint ⟂ source ⟂ confidence: no inferential bridge among accordingTo, wasAttributedTo, confidence (mirrors test_identity_orthogonality).
- `test_vantage_semantically_subsumes_according_to` — gmeow:vantage ⊑ gmeow:accordingTo is documented on the TBox (#68).
- `test_vantage_recognises_observer_as_standpoint` — The vantage agent — observer, sensor, perceiver — IS a standpoint (#68).
- `test_according_to_references_vantage_as_reified_counterpart` — accordingTo definition references vantage as its reified counterpart (#68).
- `test_no_preferred_or_primary_term_is_declared` — No GMEOW vocabulary term is a preferred/primary selector — there is no single slot to win (Principle 9).
- `test_contested_places_cannot_force_inconsistency` — Coexistence is reasoning-safe: contested containment can't make the reasoned graph inconsistent, because containedInPlace is not functional and places are not declared pairwise-disjoint.
- `test_no_frame_collapsing_projection_exists` — There is NO down-projection that selects one standpoint.
- `test_standpoint_owl2_projection_emits_tool_compatible_labels` — The lossless Standpoint-OWL 2 projection re-expresses accordingTo + standpointModality as the cl-tud/standpoint-owl2 standpointLabel encoding:
- `test_crminf_projection_is_at_least_as_expressive` — The CRMinf projection re-expresses every claim as I1 Argumentation / I2
- `test_prov_projection_attributes_every_standpoint` — The PROV-O projection makes each reified claim a prov:Entity attributed (qualifiedAttribution) to its standpoint agent — every standpoint retained, none privileged, and the proposition kept reified (never asserted).
- `test_oa_projection_annotates_each_claim` — The Web Annotation projection makes each reified claim an oa:Annotation — creator = the standpoint, target = the subject, body = the reified statement — preserving every standpoint and never asserting the proposition.
- `test_schema_projection_emits_per_standpoint_claims` — The schema.
- `test_standpoint_tenure_generates_claim_restriction` — StandpointTenure has an EL restriction requiring at least one standpointClaim.
- `test_standpoint_crminf_projection_from_standpoint_claim_reified` — Branch B: StandpointClaim with reified-statement observedFeature produces the same CRMinf structure as the annotation-form fixture.
- `test_standpoint_crminf_projection_from_standpoint_claim_entity` — Branch C: StandpointClaim with generic-entity observedFeature produces
- `test_standpoint_schema_projection_from_standpoint_claim_entity` — Branch C: schema projection renders the entity IRI as schema:text.
- `test_bbc_projection_exists` — The BBC News Ontology projection is generated and ships with the repo.
- `test_bbc_projection_emits_news_event` — A StandpointClaim about an Event produces a bbc:NewsEvent.
- `test_standpoint_claim_maps_to_crminf_i5` — SSSOM row exists for StandpointClaim → crminf:I5_Inference_Making.
- `test_standpoint_claim_maps_to_iao_assertion` — SSSOM row exists for StandpointClaim → iao:assertion.
- `test_standpoint_claim_maps_to_oa_annotation` — SSSOM row exists for StandpointClaim → oa:Annotation.
- `test_standpoint_maps_to_iptc_assertor` — SSSOM row exists for Standpoint → iptc:Assertor.
- `test_claim_modality_maps_to_sosa_has_result` — SSSOM row exists for claimModality → sosa:hasResult.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; consumer-projection round-trips exercised through the python projection harness; abox fixture instance checks; sssom mapping ledger reads through the python mapping harness.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
