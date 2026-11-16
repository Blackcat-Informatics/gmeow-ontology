# Issue #1896: Ground evidence and trust strengths without conflating confidence and probability

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: terms

## Body

Parent: #1717. Blocks: #1717. This is one complete cluster from the original quantitative-grounding contract; the parent remains open until every remaining cluster and its production acceptance are complete.

## Scope and acceptance

Audit evidence/trust strength and confidence consumers. Keep epistemic confidence on the canonical logic:/claim-confidence surface; probabilistic claims require a math: probability model. Preserve Principle 19 probability-versus-confidence and standpoint/source separation, categorical trust and authority as distinct concepts. Reuse existing grounded claim/observation owners instead of manufacturing numeric properties. Where inherently quantitative claims exist, connect real math artifacts, matching manifest dependencies and guarded retirement of superseded scalar paths. Exercise actual consumers with positive and negative confidence/probability/model controls and full required gates. Distinguish already complete inherited grounding from missing semantic links with evidence.

Principles 4, 7, 9, 11, 17 and 19 govern this delivery. No requirement is waived by this decomposition. External repository implementation or model/service lifecycle actions require separate authorization.


## Comments (2)

### paudley — 2026-11-03T12:44:04Z

A bounded source audit found substantial existing grounding and controls; adding more strength labels would not complete this issue. Existing trust quantities distinguish framed introducer weights from probability, while categorical evidence, confidence, trust and quantitative composition retain separate axes. These are source observations, not newly executed results.

A concrete remaining integration gap is visible in the selected native probabilistic-query path. crates/gmeow-dev-cli/src/dev_logic.rs:82 loads the world, parses query directives and calls probabilistic::evaluate directly. crates/logic/src/query_ir.rs:426 defines QProbModel as FullIndependence or Dependency with joint rows; it retains no selected mathematical model-artifact identity or validated lowering. crates/logic/src/probabilistic.rs:191 checks the query's declared model, rather than admitting an explicitly selected mathematical artifact into this path.

The existing test a_model_in_the_world_does_not_replace_the_query_selected_model correctly refuses to auto-adopt a model merely present in the world. Preserve that guard. The next implementation unit should explicitly select and validate an actual mathematical model, lower it through the intended query path, retain its identity and add actual-artifact positive and negative controls. It should not silently select an arbitrary catalogue model or conflate confidence with probability.

This does not establish the full consumer audit or production acceptance of the parent. No code, test, build, producer, synchronization or gate ran in this source-only audit. Existing publication and execution constraints remain unchanged. Principles 4, 7 and 17.


### paudley — 2026-11-14T23:30:51Z

Blocks: #2795 (Release 0.4.0).

