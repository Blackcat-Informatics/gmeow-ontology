# Issue #2674: Repair estimate provenance model association and data roles

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, module-enhancement, quality, logic, terms

## Body

The estimate-provenance competency query currently assigns a fitted-model-only data role to an inference activity and treats shared data as proof that a model produced an estimate. Work stays in gmeow-ontology. Principles 4, 7, 9, 17 and 19.

Source inspected at 543ba10866ab5d17a2ff9c9c0f9e81d32ce57293:

- slices/grounding/math/module.ttl reserves fittedToData for FittedModel, but the InferenceRun definition refers to that predicate as the activity's fitting link.
- slices/grounding/math/tests/fixtures/statistics-estimate-provenance.ttl attaches fittedToData directly to ex:analysisRun, an InferenceRun/Activity.
- slices/grounding/math/queries/competency/statistics-estimate-provenance.rq joins that run's data to every FittedModel using the same data. An unrelated model on that dataset therefore satisfies the model/assumption join despite no asserted producing-model association with the estimate. The current single-model fixture cannot expose this error.
- The query also selects estimatesEstimand only. It is a scoped competency query, not a general admission classifier: the canonical Estimate target contract admits an estimand OR a model parameter, plus a required estimator.

Acceptance:

- Keep fittedToData on its intended fitted-model subject. Repair the fixture, query and conflicting InferenceRun prose together without widening the predicate to activities merely to preserve the example.
- Trace the model association through actual declared provenance or a properly grounded relation. Shared input data alone must not manufacture a producing-model link. Preserve genuine ambiguity rather than claiming uniqueness or choosing a convenient model.
- Preserve the competency question's full method, target, process, data, model and assumption evidence. State whether its selected profile intentionally covers estimand-bearing estimates only; do not treat a valid parameter-only estimate as structurally invalid.
- Add a bounded adversarial fixture with another model on the same data and verify that unrelated model/assumption rows are not presented as the estimate's producing provenance. Cover the chosen ambiguity boundary when a run involves multiple models. Tests may consume existing authenticated corpus inputs or tiny synthetic data, never produce the corpus.
- Preserve process/result/held-claim separation and dataset-by-reference semantics. Do not invent a numerical fit, data payload, unsupported relationship or successful execution to make the query answerable.
- Complete focused product qualification and normal configured signed hooks. Keep broader #1656/#1717, numeric, native source-fidelity and CI requirements open. No executed reproduction or runtime pass is claimed by this source diagnosis.



## Comments (0)

