# Retention: `tests/test_standpoint.py`

**Category:** Oracle / Docker orchestration

## What it tests

The standpoint / contested-claims facility.

Retained dynamic tests:

- `test_three_axes_are_orthogonal` — standpoint ⟂ source ⟂ confidence: no inferential bridge among accordingTo, wasAttributedTo, confidence (mirrors test_identity_orthogonality).
- `test_vantage_semantically_subsumes_according_to` — gmeow:vantage ⊑ gmeow:accordingTo is documented on the TBox.
- `test_vantage_recognises_observer_as_standpoint` — The vantage agent — observer, sensor, perceiver — IS a standpoint.
- `test_according_to_references_vantage_as_reified_counterpart` — accordingTo definition references vantage as its reified counterpart.
- `test_no_preferred_or_primary_term_is_declared` — No GMEOW vocabulary term is a preferred/primary selector — there is no single slot to win.
- `test_contested_places_cannot_force_inconsistency` — Coexistence is reasoning-safe: contested containment can't make the reasoned graph inconsistent, because containedInPlace is not functional and places are not declared pairwise-disjoint.
- `test_no_frame_collapsing_projection_exists` — There is NO down-projection that selects one standpoint.
- `test_standpoint_owl2_projection_emits_tool_compatible_labels` — The lossless Standpoint-OWL 2 projection re-expresses accordingTo + standpointModality as the cl-tud/standpoint-owl2 standpointLabel encoding: Box for unequivocal (□), Diamond for conceivable (◊), the standpoint name carried, and the property IRI ending in #st.
- `test_crminf_projection_is_at_least_as_expressive` — The CRMinf projection re-expresses every claim as I1 Argumentation / I2 Belief / I4 Proposition Set with an explicit J5-holds-to-be belief value — true/possible/false — so a standpoint's DENIAL is carried first-class (GMEOW ≥ CRMinf) and the (refuted) proposit.
- `test_prov_projection_attributes_every_standpoint` — The PROV-O projection makes each reified claim a prov:Entity attributed (qualifiedAttribution) to its standpoint agent — every standpoint retained, none privileged, and the proposition kept reified (never asserted).
- `test_oa_projection_annotates_each_claim` — The Web Annotation projection makes each reified claim an oa:Annotation — creator = the standpoint, target = the subject, body = the reified statement — preserving every standpoint and never asserting the proposition.
- `test_schema_projection_emits_per_standpoint_claims` — The schema.
- `test_standpoint_tenure_generates_claim_restriction` — StandpointTenure has an EL restriction requiring at least one standpointClaim.
- `test_standpoint_crminf_projection_from_standpoint_claim_reified` — Branch B: StandpointClaim with reified-statement observedFeature produces the same CRMinf structure as the annotation-form fixture.
- `test_standpoint_crminf_projection_from_standpoint_claim_entity` — Branch C: StandpointClaim with generic-entity observedFeature produces CRMinf with crm:P67_refers_to pointing to the entity.
- `test_standpoint_schema_projection_from_standpoint_claim_entity` — Branch C: schema projection renders the entity IRI as schema:text.
- `test_bbc_projection_exists` — The BBC News Ontology projection is generated and ships with the repo.
- `test_bbc_projection_emits_news_event` — A StandpointClaim about an Event produces a bbc:NewsEvent.
- `test_standpoint_claim_maps_to_crminf_i5` — SSSOM row exists for StandpointClaim → crminf:I5_Inference_Making.
- `test_standpoint_claim_maps_to_iao_assertion` — SSSOM row exists for StandpointClaim → iao:assertion.
- `test_standpoint_claim_maps_to_oa_annotation` — SSSOM row exists for StandpointClaim → oa:Annotation.
- `test_standpoint_maps_to_iptc_assertor` — SSSOM row exists for Standpoint → iptc:Assertor.
- `test_claim_modality_maps_to_sosa_has_result` — SSSOM row exists for claimModality → sosa:hasResult.

## Why it cannot be deleted or moved to Rust today

Dynamic-set sweeps, whole-graph guards, bnode-list walks, run_shacl ExampleConformance calls, .rq projection checks, DSL checks, load_mappings SSSOM checks, and filesystem existence checks.
