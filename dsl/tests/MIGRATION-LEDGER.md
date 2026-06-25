<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Test migration ledger — rdflib pytest → declarative test-DSL (#784, T3)

This ledger records, for the representative slice migrated end-to-end in T3
(`slices/core/epistemics`), exactly which pytest tests were **converted** into
slice-resident declarative test-DSL cells (executed by the native Rust
harness, `crates/slicetest`) and which were **retained** in pytest — and why.
Nothing is silently dropped (epistemic-shape preservation): every former pytest
assertion either has a declarative twin that now executes, or an explicit
retained-with-reason row below.

The harness runs three cell types over `slices/**/tests/*.ttl`:

- `gmeow:CompetencyQuestion` — SPARQL ASK/SELECT over the merged ontology. By
  default (`gmeow:reasoningNone`) the *asserted* merged graph, with SPARQL
  property paths supplying transitive closure at query time; a question may opt
  into `gmeow:reasoningRdfs` (the merged graph closed under RDFS, computed
  natively in oxigraph). See `docs/TESTING.md` for the reasoning model.
- `gmeow:StructuralAssertion` — SPARQL ASK over the slice module (± examples).
- `gmeow:ExampleConformance` — native SHACL over module + shapes + the example.

## `slices/core/epistemics`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_knows_that_subproperty_of_believes` | `tests/test_epistemics.py` | `ex:saKnowsThatSubPropertyOfBelieves` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_are_object_properties_with_agent_domain` | `tests/test_epistemics.py` | `ex:saSpineObjectPropertiesWithAgentDomain` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_have_open_range` | `tests/test_epistemics.py` | `ex:saSpineOpenRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_proposition_is_a_social_object` | `tests/test_epistemics.py` | `ex:saPropositionIsSocialObject` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_properties_are_not_functional` | `tests/test_epistemics.py` | `ex:saSpineNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_factivity_no_truth_bit` | `tests/test_epistemics.py` | `ex:saNoTruthBit` + `ex:saSpineOpenRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_doxastic_standpoint_claim_is_subclass_of_standpoint_claim` | `tests/test_epistemics.py` | `ex:saDoxasticStandpointClaimSubclass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_claim_of_belief_is_functional_object_property` | `tests/test_epistemics.py` | `ex:saClaimOfBeliefFunctionalObjectProperty` | StructuralAssertion | converted | — | `make slicetest` |
| `test_defeated_by_has_status_range` | `tests/test_epistemics.py` | `ex:saDefeatedByStatusRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_justification_status_individuals_exist` | `tests/test_epistemics.py` | `ex:saJustificationStatusIndividuals` | StructuralAssertion | converted | — | `make slicetest` |
| `test_credence_and_confidence_are_distinct` | `tests/test_epistemics.py` | `ex:saCredenceDomainDoxasticState` + `ex:saConfidenceNotConflatedWithCredence` | StructuralAssertion | converted | — | `make slicetest` |
| `test_flagship_example_parses` | `tests/test_epistemics.py` | — | — | **retained** | the flagship references cross-slice classes; the harness's slice-scoped ExampleConformance emits `shacl.ClassConstraintComponent` for the unresolved cross-slice `sh:class` targets. Validated in full by `make validate`. | pytest |
| `test_justified_by_has_named_domain_and_range` | `tests/test_epistemics.py` | `ex:saJustifiedByDomainRange` + `ex:saJustifiedByNotFunctional` | StructuralAssertion | **partial** | the exact `owl:unionOf` set membership stays in pytest as `test_justified_by_union_membership` (an "these members and no others" check the ASK form can't express) | `make slicetest` + pytest |
| `test_justification_terms_are_annotated` | `tests/test_epistemics.py` | — | — | **retained** | universal over *dynamically discovered* `JustificationStatus` individuals (open set); backstops the make-validate annotation contract | pytest |
| `test_every_term_is_annotated` | `tests/test_epistemics.py` | — | — | **retained** | annotation-completeness sweep; backstops the make-validate annotation contract | pytest |
| `test_suppression_round_trip` | `tests/test_epistemics.py` | — | — | **retained** | numeric credence comparison + temporal tenure navigation; not expressible as a SPARQL ASK | pytest |
| `test_epistemics_mapping_set_exists_and_has_expected_rows` | `tests/test_epistemics.py` | — | — | **retained** | reads a GENERATED artifact (SSSOM TSV) outside the ontology graph | pytest |
| `test_competency_agents_query` | `tests/test_competency.py` | `ex:cqAgentKinds` | CompetencyQuestion | converted | — | `make slicetest` |
| `test_competency_contribution_roles_query` | `tests/test_competency.py` | `ex:cqContributionRoles` | CompetencyQuestion | converted | — | `make slicetest` |
| (T2: the missing-agent counter-example) | `tests/test_epistemics.py` (historical) | `ex:ecDoxasticStateMissingAgent` | ExampleConformance | converted (T2) | — | `make slicetest` |

**Epistemics tally:** 14 converted (11 structural + 2 competency + 1 T2
counter-example), 1 partial, 5 retained-with-reason.

## Measured wall-clock

The native harness runs every migrated epistemics assertion — the two competency
questions (agent kinds + all 48 contribution roles), the structural assertions,
and the example-conformance pair — under `cargo-nextest` in **0.32 s** (14 cases;
~3.2 s including the cargo build/link step). `make slicetest`.

For contrast, the pytest lane these assertions were lifted out of is dominated by
its OWL-2-RL reasoned-graph build: the retained `tests/test_competency.py` plus
`slices/core/epistemics/tests/test_epistemics.py` together run in **~10 min 22 s**
(55 tests on this worktree) — almost entirely the reasoned-graph chase
`tests/test_competency.py` pays. The native competency lane deliberately avoids
that chase (asserted graph + SPARQL property paths, the `reasoningNone` default),
which is why it is sub-second.

**Honest scope:** this PR migrates **one** representative slice (epistemics). The
gate-wide collapse of the ~1,950 rdflib ontology-data tests lands incrementally as
more slices migrate under #781 — the bulk of that suite still runs today. What
this PR banks is the end-to-end template plus the sub-second native lane every
future slice migration inherits.

## `slices/core/mentation`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_mentalprocess_subclass_of_event` | `tests/test_mentation.py` | `ex:saMentalProcessSubclassOfEvent` | StructuralAssertion | converted | — | `make slicetest` |
| `test_experience_subclass_of_mentalprocess` | `tests/test_mentation.py` | `ex:saExperienceSubclassOfMentalProcess` | StructuralAssertion | converted | — | `make slicetest` |
| `test_experiencer_is_functional_object_property` | `tests/test_mentation.py` | `ex:saExperiencerFunctionalObjectProperty` | StructuralAssertion | converted | — | `make slicetest` |
| `test_mentalprocesstype_property` | `tests/test_mentation.py` | `ex:saMentalProcessTypeProperty` + `ex:saMentalProcessTypeNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_mentalprocesstype_is_value_vocab` | `tests/test_mentation.py` | `ex:saMentalProcessTypeValueVocab` | StructuralAssertion | converted | — | `make slicetest` |
| `test_all_eight_process_individuals` | `tests/test_mentation.py` | `ex:saAllNineProcessIndividuals` | StructuralAssertion | converted | — | `make slicetest` |
| `test_realizesmentalmoment_property` | `tests/test_mentation.py` | `ex:saRealizesMentalMomentProperty` + `ex:saRealizesMentalMomentNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_producesmentalmoment_property` | `tests/test_mentation.py` | `ex:saProducesMentalMomentProperty` + `ex:saProducesMentalMomentNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_updatesmentaltenure_property` | `tests/test_mentation.py` | `ex:saUpdatesMentalTenureProperty` + `ex:saUpdatesMentalTenureNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_realizes_collision_guard` | `tests/test_mentation.py` | `ex:saRealizesAbsentFromMentation` + `ex:saBridgePropertiesDeclared` | StructuralAssertion | converted | — | `make slicetest` |
| `test_every_term_annotated` | `tests/test_mentation.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole on every gmeow:-namespaced owl:Class and property; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (e.g. the MentalProcessType individuals). Verified 2026-06-22: removing `gmeow:MentalProcess`'s `rdfs:label` caused `make validate` to emit "error class https://blackcatinformatics.ca/gmeow/MentalProcess is missing rdfs:label" and exit non-zero. | `make validate` |

**Mentation tally:** 10 converted (13 cells across 10 pytest functions), 0 retained, 1 deleted-covered-by-make-validate. Source file `tests/test_mentation.py` deleted entirely (no retained functions).

## `slices/core/inquiry`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_question_is_a_social_object_kind` | `tests/test_inquiry.py` | `ex:saQuestionIsSocialObjectKind` | StructuralAssertion | converted | — | `make slicetest` |
| `test_content_mode_siblings_have_no_subsumption` | `tests/test_inquiry.py` | `ex:saContentModeSiblingsNoSubsumption` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_are_object_properties_with_agent_domain_open_range` | `tests/test_inquiry.py` | `ex:saSpineObjectPropertiesWithAgentDomain` + `ex:saSpineOpenRange` + `ex:saSpineNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_is_flat` | `tests/test_inquiry.py` | `ex:saSpineIsFlat` | StructuralAssertion | converted | — | `make slicetest` |
| `test_question_type_is_an_abstract_individual_type` | `tests/test_inquiry.py` | `ex:saQuestionTypeIsAbstractIndividualType` | StructuralAssertion | converted | — | `make slicetest` |
| `test_question_type_individuals_are_seeded` | `tests/test_inquiry.py` | `ex:saQuestionTypeIndividualsSeeded` | StructuralAssertion | converted | — | `make slicetest` |
| `test_question_type_property` | `tests/test_inquiry.py` | `ex:saQuestionTypeProperty` + `ex:saQuestionTypeNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_presupposes_property` | `tests/test_inquiry.py` | `ex:saPresupposesProperty` + `ex:saPresupposesNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_answers_has_open_domain` | `tests/test_inquiry.py` | `ex:saAnswersProperty` + `ex:saAnswersOpenDomain` + `ex:saAnswersNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_evokes_has_open_domain` | `tests/test_inquiry.py` | `ex:saEvokesProperty` + `ex:saEvokesOpenDomain` + `ex:saEvokesNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_inquiry_tenure_is_a_mediating_situation` | `tests/test_inquiry.py` | `ex:saInquiryTenureClass` + `ex:saInquiryTenureRoles` + `ex:saInquiryTenureELRestriction` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_truth_or_resolved_bit` | `tests/test_inquiry.py` | `ex:saNoTruthOrResolvedBit` | StructuralAssertion | converted | — | `make slicetest` |
| `test_every_declared_term_is_annotated` | `tests/test_inquiry.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole on every gmeow:-namespaced owl:Class and property; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (e.g. the QuestionType individuals). Verified 2026-06-22: removing `gmeow:Question`'s `rdfs:label` caused `make validate` to emit "error Every GMEOW class must carry at least one rdfs:label." and exit non-zero. | `make validate` |

**Inquiry tally:** 12 converted (20 cells across 12 pytest functions), 0 retained, 1 deleted-covered-by-make-validate. Source file `tests/test_inquiry.py` deleted entirely (no retained functions).

## `slices/core/metacognition`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_metacognitive_state_is_a_mental_moment_kind` | `tests/test_metacognition.py` | `ex:saMetacognitiveStateIsMentalMomentKind` + `ex:saMetacognitiveStateNoExtraLogicMetaclass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_metacognitive_state_is_a_sibling_not_a_sub_mode` | `tests/test_metacognition.py` | `ex:saMetacognitiveStateSiblingNotSubMode` | StructuralAssertion | converted | — | `make slicetest` |
| `test_meta_target_is_open_range_and_characteristic_free` | `tests/test_metacognition.py` | `ex:saMetaTargetObjectPropertyWithDomain` + `ex:saMetaTargetOpenRange` + `ex:saMetaTargetFlat` + `ex:saMetaTargetCharacteristicFree` | StructuralAssertion | converted | — | `make slicetest` |
| `test_calibration_status_is_an_abstract_individual_type` | `tests/test_metacognition.py` | `ex:saCalibrationStatusIsAbstractIndividualType` | StructuralAssertion | converted | — | `make slicetest` |
| `test_calibration_statuses_are_seeded_individuals` | `tests/test_metacognition.py` | `ex:saCalibrationStatusesSeeded` + `ex:saCalibrationStatusesNotSubclasses` | StructuralAssertion | converted | — | `make slicetest` |
| `test_calibration_property` | `tests/test_metacognition.py` | `ex:saCalibrationProperty` + `ex:saCalibrationNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_calibration_error_is_a_solver_layer_annotation` | `tests/test_metacognition.py` | `ex:saCalibrationErrorIsAnnotationProperty` + `ex:saCalibrationErrorNotDataOrObjectProperty` + `ex:saCalibrationErrorNoDomainRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_known_unknown_and_self_trust_are_flat_open_range_agent_props` | `tests/test_metacognition.py` | `ex:saKnownUnknownAndSelfTrustObjectProperties` + `ex:saKnownUnknownAndSelfTrustOpenRange` + `ex:saKnownUnknownAndSelfTrustFlat` + `ex:saKnownUnknownAndSelfTrustNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_reflection_is_an_event_type_individual` | `tests/test_metacognition.py` | `ex:saEventTypeReflectionIsIndividual` + `ex:saEventTypeReflectionNotClassOrSubclass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_status_or_truth_bit` | `tests/test_metacognition.py` | `ex:saNoStatusOrTruthBit` + `ex:saNoXsdBooleanRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_bridges_are_documented_not_axiomatised` | `tests/test_metacognition.py` | `ex:saBridgesDocumentedNotAxiomatised` | StructuralAssertion | converted | — | `make slicetest` |
| `test_every_declared_term_is_annotated` | `tests/test_metacognition.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole on every gmeow:-namespaced owl:Class and property; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (e.g. CalibrationStatus individuals, eventTypeReflection). Verified 2026-06-22: removing `gmeow:MetacognitiveState`'s `rdfs:label` caused `make validate` to emit "error class https://blackcatinformatics.ca/gmeow/MetacognitiveState is missing rdfs:label" and "error Every GMEOW class must carry at least one rdfs:label." and exit non-zero. | `make validate` |

**Metacognition tally:** 11 converted (21 cells across 11 pytest functions), 0 retained, 1 deleted-covered-by-make-validate. Source file `tests/test_metacognition.py` deleted entirely (no retained functions).

## `slices/core/diagnostics`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_finding_is_a_subkind_of_observation` | `tests/test_diagnostics.py` | `ex:saFindingIsSubKindOwlClass` + `ex:saFindingNotKind` | StructuralAssertion | converted | — | `make slicetest` |
| `test_severity_and_location_subproperty_observation_roles` | `tests/test_diagnostics.py` | `ex:saFindingSeveritySubPropertyAndRange` + `ex:saFindingLocationSubProperty` + `ex:saFindingLocationOpenRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_diagnostic_severity_is_a_value_vocabulary` | `tests/test_diagnostics.py` | `ex:saDiagnosticSeverityIsValueVocab` + `ex:saSeverityIndividualsSeeded` + `ex:saSeverityIndividualsNotClasses` | StructuralAssertion | converted | — | `make slicetest` |
| `test_wire_coordinates_are_datatype_properties` | `tests/test_diagnostics.py` | `ex:saWireCoordinatesAreDatatype` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_truth_or_resolution_bits` | `tests/test_diagnostics.py` | `ex:saNoTruthOrResolutionBits` | StructuralAssertion | converted | — | `make slicetest` |
| `test_annotation_completeness` | `tests/test_diagnostics.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` + `GmeowDatatypeShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy on every gmeow:-namespaced owl:Class, property, and rdfs:Datatype; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (e.g. DiagnosticSeverity individuals). Verified 2026-06-22: removing `gmeow:Finding`'s `rdfs:label` caused `make validate` to emit "error Every GMEOW class must carry at least one rdfs:label." and exit non-zero. | `make validate` |
| `test_graph_box_role_coverage` | `tests/test_diagnostics.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` / `GmeowPropertyShape` / `GmeowDatatypeShape` (shapes/gmeow-shapes.ttl) also carry `sh:path gmeow:graphBoxRole` min-count 1 for owl:Class, property, and rdfs:Datatype nodes (verified 2026-06-22: the graphBoxRole constraint is wired alongside the label constraint in the same shapes); (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the graphBoxRole contract on value-vocab individuals (e.g. DiagnosticSeverity individuals) — the SHACL shapes do NOT target individuals. Both annotation and graphBoxRole invariants therefore fire together under `make validate` for the full term set. | `make validate` |

**Diagnostics tally:** 5 converted (9 cells across 5 pytest functions), 0 retained, 2 deleted-covered-by-make-validate. Source file `tests/test_diagnostics.py` deleted entirely (no retained functions).

## `slices/core/awareness`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_vocab_classes_are_abstract_individual_types` | `tests/test_awareness.py` | `ex:saVocabClassesAreAbstractIndividualTypes` | StructuralAssertion | converted | — | `make slicetest` |
| `test_mode_individuals_are_seeded` | `tests/test_awareness.py` | `ex:saModeIndividualsAreSeeded` | StructuralAssertion | converted | — | `make slicetest` |
| `test_level_individuals_are_seeded` | `tests/test_awareness.py` | `ex:saLevelIndividualsAreSeeded` | StructuralAssertion | converted | — | `make slicetest` |
| `test_vocab_individuals_are_not_subclasses` | `tests/test_awareness.py` | `ex:saVocabIndividualsNotClasses` + `ex:saVocabIndividualsNotSubclasses` | StructuralAssertion | converted | — | `make slicetest` |
| `test_property_types_and_ranges` | `tests/test_awareness.py` | `ex:saPropertyTypesAndRanges` | StructuralAssertion | converted | — | `make slicetest` |
| `test_awareness_edges_open_domain` | `tests/test_awareness.py` | `ex:saAwarenessEdgesOpenDomain` | StructuralAssertion | converted | — | `make slicetest` |
| `test_awareness_subject_bearer_edge` | `tests/test_awareness.py` | `ex:saAwarenessSubjectBearerEdge` | StructuralAssertion | converted | — | `make slicetest` |
| `test_awareness_tenure_is_a_time_scoped_situation` | `tests/test_awareness.py` | `ex:saAwarenessTenureTimeScopedSituation` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_reality_or_truth_bit` | `tests/test_awareness.py` | `ex:saNoRealityOrTruthBit` | StructuralAssertion | converted | — | `make slicetest` |
| `test_by_reference_no_inherence_triple` | `tests/test_awareness.py` | `ex:saByReferenceNoInherenceTriple` | StructuralAssertion | converted | — | `make slicetest` |
| `test_level_ranks_are_zero_through_five` | `tests/test_awareness.py` | — | — | **retained** | closed numeric SET-EQUALITY over the six `gmeow:levelRank` values: a SPARQL ASK can assert each rank exists but not that the set is EXACTLY `{0,1,2,3,4,5}` and no others | pytest |
| `test_manifest_depends_only_on_kernel_and_temporal` | `tests/test_awareness.py` | — | — | **retained** | set-equality over `manifest.ttl`, which `run_structural_cell` never loads (store = module.ttl + examples/ only) | pytest |
| `test_every_declared_term_is_annotated` | `tests/test_awareness.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole on every gmeow:-namespaced owl:Class and property; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (the `mode*` / `level*` individuals). Verified 2026-06-22, BOTH guardians exercised: removing `gmeow:levelAlert`'s `rdfs:label` (a value-vocab INDIVIDUAL) made `make validate` emit "error individual https://blackcatinformatics.ca/gmeow/levelAlert is missing rdfs:label" (the Rust guardian) and exit non-zero; removing `gmeow:AwarenessTenure`'s `rdfs:label` (a CLASS) made it emit "error class https://blackcatinformatics.ca/gmeow/AwarenessTenure is missing rdfs:label" (the SHACL guardian) and exit non-zero. | `make validate` |

**Awareness tally:** 10 converted (11 cells across 10 pytest functions), 2 retained-with-reason (numeric set-equality + manifest-scope), 1 deleted-covered-by-make-validate. Source file `tests/test_awareness.py` trimmed to the 2 retained functions (not deleted — the must-stays remain).

## `slices/core/imagination`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained/deleted | Run by |
|---|---|---|---|---|---|---|
| `test_spine_are_object_properties_with_agent_domain_open_range` | `tests/test_imagination.py` | `ex:saSpineObjectPropertiesAgentDomain` + `ex:saSpineOpenRangeNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_spine_is_flat_and_decoupled` | `tests/test_imagination.py` | `ex:saSpineFlatAndDecoupled` | StructuralAssertion | converted | — | `make slicetest` |
| `test_content_origin_is_an_abstract_individual_type` | `tests/test_imagination.py` | `ex:saContentOriginAbstractIndividualType` | StructuralAssertion | converted | — | `make slicetest` |
| `test_content_origin_individuals_are_seeded` | `tests/test_imagination.py` | `ex:saContentOriginIndividualsSeeded` | StructuralAssertion | converted | — | `make slicetest` |
| `test_content_origin_individuals_are_not_subclasses` | `tests/test_imagination.py` | `ex:saContentOriginIndividualsNotSubclasses` | StructuralAssertion | converted | — | `make slicetest` |
| `test_content_origin_property_open_domain` | `tests/test_imagination.py` | `ex:saContentOriginPropertyRange` + `ex:saContentOriginPropertyOpenDomain` | StructuralAssertion | converted | — | `make slicetest` |
| `test_imagined_world_open_domain_and_range` | `tests/test_imagination.py` | `ex:saImaginedWorldObjectProperty` + `ex:saImaginedWorldOpenDomainAndRange` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_reality_or_truth_bit` | `tests/test_imagination.py` | `ex:saNoRealityOrTruthBit` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_new_content_class` | `tests/test_imagination.py` | `ex:saNoNewContentClass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_by_reference_no_logic_triples` | `tests/test_imagination.py` | `ex:saByReferenceNoLogicTriples` | StructuralAssertion | converted | — (DYNAMIC FILTER over every `logic:` node, allowing only the 15 #694 stereotype IRIs — NOT a hand-listed blacklist) | `make slicetest` |
| `test_manifest_depends_only_on_kernel` | `tests/test_imagination.py` | — | — | **retained** | set-equality over `manifest.ttl`, which `run_structural_cell` never loads (store = module.ttl + examples/ only) | pytest |
| `test_every_declared_term_is_annotated` | `tests/test_imagination.py` | — | — | **deleted** | Covered by global `make validate` gate via two guardians: (1) SHACL `GmeowClassShape` + `GmeowPropertyShape` (shapes/gmeow-shapes.ttl) enforce rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole on every gmeow:-namespaced owl:Class and property; (2) the Rust `structural_lint` vocabulary-individual sweep (crates/validate/src/lint.rs `collect_typed_terms`, pinned by test `structural_still_flags_vocabulary_individual`) enforces the same contract on value-vocab individuals (the `origin*` ContentOrigin individuals). Verified 2026-06-22, BOTH guardians exercised on imagination terms: removing `gmeow:originImagined`'s `rdfs:label` (a value-vocab INDIVIDUAL) made `make validate` emit "error individual https://blackcatinformatics.ca/gmeow/originImagined is missing rdfs:label" (the Rust guardian) and exit non-zero; removing `gmeow:ContentOrigin`'s `rdfs:label` (a CLASS) made it emit "error class https://blackcatinformatics.ca/gmeow/ContentOrigin is missing rdfs:label" (the SHACL guardian) and exit non-zero. | `make validate` |

**Imagination tally:** 10 converted (13 cells across 10 pytest functions), 1 retained-with-reason (manifest-scope), 1 deleted-covered-by-make-validate. Source file `tests/test_imagination.py` trimmed to the 1 retained function (not deleted — the must-stay remains).

## `slices/core/temporal`

The temporal pytest loaded the FULL merged graph (`load_merged_graph(include_imports=True)`), so each tested triple needs home-slice triage: a triple asserted in temporal/module.ttl → `scopeModule` cell; one asserted in another slice → cross-slice must-stay.

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_time_scoped_relation_is_a_logic_situation` | `tests/test_temporal.py` | `ex:saTimeScopedRelationIsLogicSituation` | StructuralAssertion | converted | — | `make slicetest` |
| `test_validity_predicates_are_annotation_properties` | `tests/test_temporal.py` | `ex:saValidityPredicatesAreAnnotationProperties` | StructuralAssertion | converted | — | `make slicetest` |
| `test_instant_subclasses_logic_abstract_individual` | `tests/test_temporal.py` | `ex:saInstantSubclassesLogicAbstractIndividual` | StructuralAssertion | converted | — | `make slicetest` |
| `test_time_interval_has_start_and_end_instants` | `tests/test_temporal.py` | `ex:saTimeIntervalHasStartAndEndInstants` | StructuralAssertion | converted | — | `make slicetest` |
| `test_time_interval_can_have_temporal_frame` | `tests/test_temporal.py` | `ex:saTimeIntervalCanHaveTemporalFrame` | StructuralAssertion | converted | — | `make slicetest` |
| `test_temporal_measurement_is_logic_relator` | `tests/test_temporal.py` | `ex:saTemporalMeasurementIsLogicRelator` | StructuralAssertion | converted | — | `make slicetest` |
| `test_reified_residence_and_tenure_are_time_scoped` | `tests/test_temporal.py` | — | — | **retained** | CROSS-SLICE: `gmeow:MailboxResidence` (extensions/email) + `gmeow:AddressTenure` (core/contacts) ⊑ TimeScopedRelation — the subclass edges are declared in those OTHER slices' modules, absent from temporal/module.ttl, so the module-scoped harness cannot see them. Faithful only over the merged graph. | pytest |
| `test_interpersonal_relationship_is_a_gufo_relator` | `tests/test_temporal.py` | `slices/core/contacts/tests/structural.ttl` `ex:saInterpersonalRelationshipIsRelatorKind` | StructuralAssertion | converted | — | `make slicetest` |

**Temporal tally:** 7 converted, 1 retained-with-reason (cross-slice merged-graph). Source file `tests/test_temporal.py` trimmed to the 1 retained function (not deleted).

## `slices/core/gts`

The gts pytest mixed merged-graph, module-only, and competency loads. Subjects of the migrated subClassOf/subPropertyOf edges are home-asserted in gts/module.ttl (verified), so they convert even though the parent classes live in other slices. The two dynamic universals use `FILTER NOT EXISTS` over a type pattern (not a VALUES blacklist); adversarially break-probed 2026-06-23 (a TransformCodec without codecClass, an untyped gmeow term, a stray OpacityReason → the three cells reded as `mustNot but the ASK pattern HELD`, then reverted).

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_artifact_classes_ground_in_existing_spine` | `tests/test_gts_slice.py` | `ex:saArtifactClassesGroundInSpine` | StructuralAssertion | converted | — | `make slicetest` |
| `test_head_id_is_a_version_fingerprint` | `tests/test_gts_slice.py` | `ex:saHeadIdIsVersionFingerprint` | StructuralAssertion | converted | — | `make slicetest` |
| `test_structure_properties_are_part_of_spine` | `tests/test_gts_slice.py` | `ex:saStructurePropertiesArePartOfSpine` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_parallel_signature_or_digest_mechanism` | `tests/test_gts_slice.py` | `ex:saNoParallelSignatureOrDigest` | StructuralAssertion | converted | — | `make slicetest` |
| `test_every_codec_carries_a_codec_class` | `tests/test_gts_slice.py` | `ex:saEveryCodecCarriesACodecClass` | StructuralAssertion | converted (DYNAMIC `FILTER NOT EXISTS`, scopeModule-narrowed vs the pytest's merged scan — faithful for slice-owned codecs) | — | `make slicetest` |
| `test_slice_terms_are_class_or_property_typed` | `tests/test_gts_slice.py` | `ex:saSliceTermsAreClassOrPropertyTyped` | StructuralAssertion | converted (DYNAMIC typing sweep, `FILTER NOT EXISTS` over the allowed-type set + individual escape) | — | `make slicetest` |
| `test_value_vocabularies_are_seeded` | `tests/test_gts_slice.py` | `ex:saValueVocabNamedIndividualsSeeded` + `ex:saOpacityReasonIndividualsSeeded` + `ex:saOpacityReasonExactClosedSet` | StructuralAssertion | **partial** | converted: the named individuals (gtsProfileDist/Evidence/AiPackage, codecZstd) + the OpacityReason EXACT closed set (`mustNot FILTER ?r NOT IN (the 3)`). RETAINED in `test_value_vocabulary_cardinality_floors`: the `>=7` GTSProfile, `>=7` TransformCodec, `==3` CodecClass numeric counts — a boolean ASK cannot assert a cardinality. | `make slicetest` + pytest |
| `test_competency_queries_parse_and_run` | `tests/test_gts_slice.py` | — | — | **retained** | a parse+execute SMOKE over `queries/*.rq` with NO pinned expected result; a `gmeow:CompetencyQuestion` cell requires an expected outcome, so authoring one would fabricate an assertion the pytest never made. | pytest |

**GTS tally:** 6 converted + 1 partial (value-vocab: named/exact-set converted, cardinality floors retained) + 1 retained (competency parse-smoke); 9 cells across 7 migrated fns. Source file `tests/test_gts_slice.py` trimmed to the 2 retained functions (not deleted).

## `slices/core/concepts`

The first slice migrated using BOTH cell types: `gmeow:StructuralAssertion` (6 structural fns → `tests/structural.ttl`) AND `gmeow:ExampleConformance` (6 SHACL fns → `tests/example-conformance.ttl`, the epistemics-exemplar type). All 13 fns migrate, so `tests/test_concepts.py` is DELETED entirely. The 6 SHACL fns built inline instance graphs asserting an `sh:message`; they are migrated to 6 materialized self-contained fixtures (2 conforming under `tests/conformance-fixtures/`, 4 violating under `tests/counter-examples/`) bound to ExampleConformance cells. MESSAGE→CODE shift (epistemics precedent): cells pin the constraint-component CODE, which collapses distinct `sh:message` strings sharing a component (two MinCount messages become indistinguishable), but each counter-example isolates ONE violation so (fixture, code) is faithful. Every code was VERIFIED against the native validator AND confirmed a SINGLETON (sentinel-flip read of the full code-set), 2026-06-23.

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if deleted | Run by |
|---|---|---|---|---|---|---|
| `test_concept_is_social_object_kind` | `tests/test_concepts.py` | `ex:saConceptIsSocialObjectKind` | StructuralAssertion | converted | — | `make slicetest` |
| `test_concept_categorization_subkind_of_standpoint_claim` | `tests/test_concepts.py` | `ex:saConceptCategorizationSubkindOfStandpointClaim` | StructuralAssertion | converted | — | `make slicetest` |
| `test_instance_of_concept_property` | `tests/test_concepts.py` | `ex:saInstanceOfConceptProperty` + `ex:saInstanceOfConceptNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_typicality_property` | `tests/test_concepts.py` | `ex:saTypicalityProperty` + `ex:saTypicalityNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_concept_structure_properties` | `tests/test_concepts.py` | `ex:saConceptStructureProperties` + `ex:saConceptStructurePropertiesNotFunctionalNotTransitive` | StructuralAssertion | converted | — | `make slicetest` |
| `test_concept_tenure_is_time_scoped` | `tests/test_concepts.py` | `ex:saConceptTenureIsTimeScoped` | StructuralAssertion | converted | — | `make slicetest` |
| `test_wellformed_concept_categorization_conforms` | `tests/test_concepts.py` | `ex:ecCategorizationConforms` | ExampleConformance | converted | — | `make slicetest` |
| `test_categorization_missing_feature_is_flagged` | `tests/test_concepts.py` | `ex:ecCategorizationMissingFeature` | ExampleConformance | converted (violates `shacl.MinCountConstraintComponent`, verified singleton) | — | `make slicetest` |
| `test_categorization_result_not_concept_is_flagged` | `tests/test_concepts.py` | `ex:ecCategorizationResultNotConcept` | ExampleConformance | converted (violates `shacl.ClassConstraintComponent`, verified singleton) | — | `make slicetest` |
| `test_categorization_typicality_out_of_range_is_flagged` | `tests/test_concepts.py` | `ex:ecCategorizationTypicalityOutOfRange` | ExampleConformance | converted (violates `shacl.MaxInclusiveConstraintComponent`, verified singleton) | — | `make slicetest` |
| `test_wellformed_concept_tenure_conforms` | `tests/test_concepts.py` | `ex:ecTenureConforms` | ExampleConformance | converted | — | `make slicetest` |
| `test_tenure_missing_interval_is_flagged` | `tests/test_concepts.py` | `ex:ecTenureMissingInterval` | ExampleConformance | converted (violates `shacl.MinCountConstraintComponent`, verified singleton) | — | `make slicetest` |
| `test_every_declared_term_is_annotated` | `tests/test_concepts.py` | — | — | **deleted** | Covered by the global `make validate` gate: all 8 concepts terms are classes/properties (no value-vocab individuals), so SHACL `GmeowClassShape` / `GmeowPropertyShape` enforce the rdfs:label / skos:definition / rdfs:isDefinedBy triad. Verified 2026-06-23, BOTH shapes exercised: deleting `gmeow:Concept`'s `rdfs:label` (a CLASS) → "error class … is missing rdfs:label"; deleting `gmeow:typicality`'s `rdfs:label` (a PROPERTY) → "error property … is missing rdfs:label"; both reverted. | `make validate` |

**Concepts tally:** 12 converted (9 structural cells + 6 example-conformance cells across 12 fns), 1 deleted-covered-by-make-validate. Source file `tests/test_concepts.py` DELETED entirely (all 13 fns migrated/subsumed, no must-stays). First slice to exercise the `gmeow:ExampleConformance` cell type beyond the epistemics exemplar.

## `slices/core/learning`

The second SHACL-conformance slice (concepts recipe): 8 structural fns → `tests/structural.ttl`, 3 SHACL fns → `tests/example-conformance.ttl` + 3 materialized fixtures, 1 annotation fn deleted. All 12 fns migrate → `tests/test_learning.py` DELETED. The EL `someValuesFrom` relator-mediation restrictions (teacher + learner) are migrated as a blank-node bracket ASK faithful to the pytest's existential restriction walk. MESSAGE→CODE shift (collapses distinct messages sharing a component); every code VERIFIED + confirmed SINGLETON via sentinel-flip, 2026-06-23.

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if deleted | Run by |
|---|---|---|---|---|---|---|
| `test_learning_event_reparents_mental_process` | `tests/test_learning.py` | `ex:saLearningEventReparentsMentalProcess` | StructuralAssertion | converted | — | `make slicetest` |
| `test_process_learning_marker_rides_instances_not_the_class` | `tests/test_learning.py` | `ex:saProcessLearningMarkerRidesInstances` | StructuralAssertion | converted | — | `make slicetest` |
| `test_learning_event_type_is_an_abstract_individual_type` | `tests/test_learning.py` | `ex:saLearningEventTypeIsAbstractIndividualType` | StructuralAssertion | converted | — | `make slicetest` |
| `test_learning_event_type_individuals_are_seeded` | `tests/test_learning.py` | `ex:saLearningEventTypeIndividualsSeeded` + `ex:saLearningEventTypeIndividualsNotSubclasses` | StructuralAssertion | converted | — | `make slicetest` |
| `test_learning_type_property` | `tests/test_learning.py` | `ex:saLearningTypeProperty` + `ex:saLearningTypeNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_provenance_trajectory_product_are_open_range` | `tests/test_learning.py` | `ex:saProvenanceTrajectoryProductProperties` + `ex:saProvenanceTrajectoryProductOpenRangeNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_teaching_is_a_mediating_relator` | `tests/test_learning.py` | `ex:saTeachingIsMediatingRelator` + `ex:saLearnerNotFunctional` + `ex:saTeachingELRestrictions` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_forgetting_or_truth_bit_and_remembers_not_redeclared` | `tests/test_learning.py` | `ex:saNoForgettingOrTruthBit` | StructuralAssertion | converted | — | `make slicetest` |
| `test_wellformed_teaching_conforms` | `tests/test_learning.py` | `ex:ecTeachingConforms` | ExampleConformance | converted | — | `make slicetest` |
| `test_teacher_equals_learner_is_flagged` | `tests/test_learning.py` | `ex:ecTeachingSelf` | ExampleConformance | converted (violates `shacl.SPARQLConstraintComponent`, verified singleton — the `sh:sparql` self-teaching rule) | — | `make slicetest` |
| `test_non_agent_learner_is_flagged` | `tests/test_learning.py` | `ex:ecTeachingNonAgentLearner` | ExampleConformance | converted (violates `shacl.ClassConstraintComponent`, verified singleton — learner `sh:class gmeow:Agent`) | — | `make slicetest` |
| `test_every_declared_term_is_annotated` | `tests/test_learning.py` | — | — | **deleted** | Covered by the global `make validate` gate via TWO guardians: SHACL `GmeowClassShape` / `GmeowPropertyShape` for the classes/properties AND the Rust `structural_lint` vocabulary-individual sweep for the 6 `LearningEventType` value individuals. Verified 2026-06-23, BOTH exercised: deleting `gmeow:learningSkillAcquisition`'s `rdfs:label` (a value-vocab INDIVIDUAL) → "error individual … is missing rdfs:label" (Rust); deleting `gmeow:LearningEvent`'s `rdfs:label` (a CLASS) → "error class … is missing rdfs:label" (SHACL); both reverted. | `make validate` |

**Learning tally:** 11 converted (11 structural cells + 3 example-conformance cells across 11 fns), 1 deleted-covered-by-make-validate. Source file `tests/test_learning.py` DELETED entirely (all 12 fns migrated/subsumed, no must-stays). Second slice exercising `gmeow:ExampleConformance`; first to migrate an EL `someValuesFrom` restriction (blank-node ASK) and an `sh:sparql` constraint violation.

## `slices/core/quality`

The quality-module TBox structural assertions (#99) migrated to
`slices/core/quality/tests/structural.ttl`. The whole-ontology Principle-9 sweep is
RETAINED in pytest (a module-scoped cell would silently narrow its subject set).

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_quality_assessment_class_structure` | `tests/test_quality.py` | `ex:saQualityAssessmentIsSubKindObservation` | StructuralAssertion | converted | — | `make slicetest` |
| `test_quality_dimension_class_structure` | `tests/test_quality.py` | `ex:saQualityDimensionIsValueVocab` | StructuralAssertion | converted | — | `make slicetest` |
| `test_assessed_entity_property_structure` | `tests/test_quality.py` | `ex:saAssessedEntityProperty` | StructuralAssertion | converted | — | `make slicetest` |
| `test_quality_dimension_property_structure` | `tests/test_quality.py` | `ex:saQualityDimensionProperty` + `ex:saQualityDimensionNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_dimension_seeds_exist` | `tests/test_quality.py` | `ex:saQualityDimensionSeeds` | StructuralAssertion | converted | — | `make slicetest` |
| `test_no_preferred_or_primary_term_is_declared` | `tests/test_quality.py` | — | — | **retained** | whole-ontology Principle-9 dynamic sweep over the ENTIRE merged graph's subject set — a quality-module-scoped cell would silently narrow it (the #869 Gap-1 trap class). Kept as a dynamic-set sweep | pytest |

**Quality tally:** 5 converted (6 structural cells across 5 fns), 1 retained-with-reason. `tests/test_quality.py` keeps only the whole-ontology sweep.

## `slices/core/observations`

The observation-module asserted-TBox structural assertions (#66, #69) migrated to
`slices/core/observations/tests/structural.ttl`. The SOSA/AFO mapping tests
(generated-artifact reads) and the cross-slice KinRelationship bridges are retained.

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_observation_class_exists` | `tests/test_observations.py` | `ex:saObservationClass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_observation_properties_exist` | `tests/test_observations.py` | `ex:saObservationProperties` | StructuralAssertion | converted | — | `make slicetest` |
| `test_observation_value_vocabularies_exist` | `tests/test_observations.py` | `ex:saObservationValueVocabularies` | StructuralAssertion | converted | — | `make slicetest` |
| `test_observation_type_seeds_exist` | `tests/test_observations.py` | `ex:saObservationTypeSeeds` | StructuralAssertion | converted | — | `make slicetest` |
| `test_observation_method_seeds_exist` | `tests/test_observations.py` | `ex:saObservationMethodSeeds` | StructuralAssertion | converted | — | `make slicetest` |
| `test_scalar_quantity_properties_exist` | `tests/test_observations.py` | `ex:saScalarQuantityProperties` | StructuralAssertion | converted | — | `make slicetest` |
| `test_property_bridges_fire` | `tests/test_observations.py` | `ex:saPropertyBridges` | StructuralAssertion | **partial** | the 8 observations-module-local bridges convert; the 3 KinRelationship bridges (`relationshipParent`/`relationshipChild`/`hasPartner`) are asserted in the genealogy module → retained as `test_kin_relationship_bridges_fire` (cross-slice, pending a genealogy migration) | `make slicetest` + pytest |
| `test_quantity_equivalent_to_scalar_quantity` | `tests/test_observations.py` | `ex:saQuantityEquivalentScalarQuantity` | StructuralAssertion | converted | — | `make slicetest` |
| `test_measured_value_equivalent_to_quantity` | `tests/test_observations.py` | `ex:saMeasuredValueEquivalentQuantity` | StructuralAssertion | converted | — | `make slicetest` |
| `test_is_result_of_is_inverse_of_observation_result` | `tests/test_observations.py` | `ex:saIsResultOfInverseObservationResult` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_class_exists` | `tests/test_observations.py` | `ex:saStreamClass` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_properties_exist` | `tests/test_observations.py` | `ex:saStreamProperties` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_of_is_functional` | `tests/test_observations.py` | `ex:saStreamOfFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_has_stream_is_inverse_of_stream_of` | `tests/test_observations.py` | `ex:saHasStreamInverseStreamOf` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_sample_is_non_functional` | `tests/test_observations.py` | `ex:saStreamSampleShape` + `ex:saStreamSampleNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_platform_is_non_functional` | `tests/test_observations.py` | `ex:saStreamPlatformShape` + `ex:saStreamPlatformNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_sensor_is_non_functional` | `tests/test_observations.py` | `ex:saStreamSensorShape` + `ex:saStreamSensorNotFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_stream_interval_is_functional` | `tests/test_observations.py` | `ex:saStreamIntervalFunctional` | StructuralAssertion | converted | — | `make slicetest` |
| `test_streaming_observation_type_exists` | `tests/test_observations.py` | `ex:saStreamingObservationType` | StructuralAssertion | converted | — | `make slicetest` |
| `test_streaming_method_exists` | `tests/test_observations.py` | `ex:saStreamingMethod` | StructuralAssertion | converted | — | `make slicetest` |
| `test_standpoint_claim_aligned_to_sosa_observation` | `tests/test_observations.py` | — | — | **retained** | reads the GENERATED SSSOM mapping artifact (`load_mappings`) — independent Python surface, no module graph | pytest |
| `test_agent_aligned_to_sosa_sensor_as_standpoint` | `tests/test_observations.py` | — | — | **retained** | generated SSSOM mapping read | pytest |
| `test_coordinate_observation_mapped_to_sosa` | `tests/test_observations.py` | — | — | **retained** | generated SSSOM mapping read | pytest |
| `test_spatial_measurement_mapped_to_sosa` | `tests/test_observations.py` | — | — | **retained** | generated SSSOM mapping read | pytest |
| `test_kin_relationship_bridges_fire` (new, the cross-slice remnant of `test_property_bridges_fire`) | `tests/test_observations.py` | — | — | **retained** | the 3 KinRelationship bridges are asserted in the genealogy module, invisible to a module-scoped observations cell; over the merged graph pending a genealogy migration | pytest |

**Observations tally:** 19 converted + 1 partial (property-bridges: 8 of 11 converted) = 23 structural cells, 5 retained-with-reason (4 generated-SSSOM-mapping reads + 1 cross-slice kin-bridge). `tests/test_observations.py` 24 → 5 fns.

## #867 run_shacl → whole-ontology native conformance harness (crates/validate/tests/ontology_conformance.rs)

The `run_shacl` pytest cluster ran full-ontology SHACL validation by assembling a merged
shapes corpus in Python (`_shapes_turtle`) and feeding it alongside a fixture or inline
graph to `gmeow_validate.run_shacl`. The Rust twin at
`crates/validate/tests/ontology_conformance.rs` replicates every step at the crate level
so findings are byte-identical to Python (same `gmeow-shacl` engine underneath).

**Merged-shapes recipe** (mirrors `_shapes_turtle` exactly):

1. `shapes/gmeow-shapes.ttl` first
2. Every other `shapes/*.ttl` sorted, excluding `mapping-dsl-shapes.ttl`,
   `statement-dsl-shapes.ttl`, `test-dsl-shapes.ttl`, `slice-manifest-shapes.ttl`,
   `gmeow-shapes.ttl`
3. Every `generated/shapes/*.ttl` sorted — hard-fail (panic) if directory is empty
4. Every `slices/*/shapes.ttl` discovered by recursive walk, sorted

The corpus is assembled once per process into an `OnceLock<String>` so disk I/O is not
repeated across tests. Shapes parsing and the merged data graph are the two separate
inputs to `gmeow_validate::validate_graphs(data_nt, shapes_ttl)`.

**Two base validation modes:**

- `validate(data_nt)` — shapes from the merged corpus above, data from any NT string
- Fixture helpers `fixture_as_nt(subdir, name)` / `ttl_file_to_nt(path)` convert `.ttl`
  fixture files to NT on the fly via oxigraph (no Python round-trip)

**ok() vs conforms:** Python's `result.ok` is `not result.errors` — Violation-severity
only. SHACL's `conforms` is `false` for Warning-level results too. The `ok()` helper
captures the Python semantic (Violation-free = ok), making suppression/warning-only
tests expressible cleanly.

This harness is the foundational substrate for migrating the remaining ~230
`run_shacl` call sites in later batches.

### Batch 1: test_shapes.py fixture-backed + inline-graph tests

| pytest fn | source file | disposition | Rust twin test name | note |
|-----------|-------------|------------|---------------------|------|
| `test_wellformed_relator_fixture_conforms` | `tests/test_shapes.py` | converted | `wellformed_relator_fixture_conforms` | |
| `test_malformed_relator_fixture_is_flagged` | `tests/test_shapes.py` | converted | `malformed_relator_fixture_is_flagged` | all 4 substrings asserted |
| `test_suppression_warning_does_not_fail_validation` | `tests/test_shapes.py` | converted | `suppression_warning_does_not_fail_validation` | Warning-only; ok() must be true |
| `test_orthogonality_data_check_rejects_two_axes` | `tests/test_shapes.py` | converted | `orthogonality_data_check_rejects_two_axes` | inline Turtle |
| `test_wellformed_facet_cardinality_passes` | `tests/test_shapes.py` | converted | `wellformed_facet_cardinality_passes` | inline Turtle |
| `test_internal_language_tag_shape_is_case_insensitive` | `tests/test_shapes.py` | converted | `internal_language_tag_shape_is_case_insensitive` | inline Turtle |
| `test_wellformed_reference_frame_passes` | `tests/test_shapes.py` | converted | `wellformed_reference_frame_passes` | inline Turtle |
| `test_reference_frame_axis_count_must_match_dimension_count` | `tests/test_shapes.py` | converted | `reference_frame_axis_count_must_match_dimension_count` | inline Turtle |
| `test_malformed_reference_frame_fails` | `tests/test_shapes.py` | converted | `malformed_reference_frame_fails` | inline Turtle |
| `test_profile_open_value_guard_warns_on_orphan` | `tests/test_shapes.py` | converted | `profile_open_value_guard_warns_on_orphan` | inline Turtle |
| `test_wellformed_proximity_fixture_conforms` | `tests/test_shapes.py` | converted | `wellformed_proximity_fixture_conforms` | |
| `test_malformed_proximity_fixture_is_flagged` | `tests/test_shapes.py` | converted | `malformed_proximity_fixture_is_flagged` | |
| `test_wellformed_expertise_fixture_conforms` | `tests/test_shapes.py` | converted | `wellformed_expertise_fixture_conforms` | |
| `test_malformed_expertise_fixture_is_flagged` | `tests/test_shapes.py` | converted | `malformed_expertise_fixture_is_flagged` | |
| `test_contested_attestation_coexists` | `tests/test_attestation.py` | converted | `contested_attestation_coexists` | |
| `test_all_fixture_files_load` | `tests/test_attestation.py` | converted | `attestation_all_fixture_files_load` | walks fixtures/attestation/ |
| `test_authority_link_without_match_strength_warns_only` | `tests/test_coreference.py` | converted | `authority_link_without_match_strength_warns_only` | Warning-only |

**Tally:** 17 converted, 0 retained. `tests/test_shapes.py` 14 → 1 fn (nodeshape collision inline test retained — no obvious Rust twin yet for that inline pattern; marked for batch 2). `tests/test_attestation.py` reduced. `tests/test_coreference.py` reduced.

Slice-shapes glob: `slices/*/shapes.ttl` discovered recursively via directory walk from repo root, sorted ascending.

### Batch 2: shared support module + test_deception.py

**Refactor:** All non-`#[test]` helpers from `ontology_conformance.rs` extracted into
`crates/validate/tests/conformance_support/mod.rs` and made `pub`. This module also adds
two merged-ontology helpers:

- `base_ontology_nt() -> &'static str` — OnceLock-cached: parses every `slices/*/*/module.ttl`
  (recursively) into one oxigraph Store and dumps as N-Triples. Mirrors
  `load_merged_graph(include_imports=False)`.
- `validate_with_ontology(fixture_nt: &str) -> ValidationReport` — validates
  `base_ontology_nt() + "\n" + fixture_nt` against `whole_shapes_ttl()` for tests that
  require class/property declarations from the merged ontology.

`ontology_conformance.rs` now contains only `mod conformance_support; use conformance_support::*;`
plus the 17 `#[test]` functions; all helper bodies deleted. All 17 tests still pass.

**Deception migration** (`tests/test_deception.py` → `crates/validate/tests/conformance_deception.rs`):

`_doxastic_claim(g, claim, agent, proposition, method)` is expanded inline as `doxastic_claim_ttl(claim, state, agent, prop, method) -> String` producing the same 7 triples (explicit double-typing of `DoxasticStandpointClaim` + `StandpointClaim` preserved).

| pytest fn | source file | disposition | Rust twin test name | note |
|-----------|-------------|------------|---------------------|------|
| `test_standpoint_divergence_coexists` | `tests/test_deception.py` | converted | `standpoint_divergence_coexists` | inline Turtle, `_doxastic_claim` expanded ×2 |
| `test_deception_event_shacl_passes` | `tests/test_deception.py` | converted | `deception_event_shacl_passes` | inline Turtle, deceptionCue observation included |
| `test_deception_cue_shacl_passes` | `tests/test_deception.py` | converted | `deception_cue_shacl_passes` | inline Turtle, identical graph to above |
| `test_paltering_implicates_structure` | `tests/test_deception.py` | converted | `paltering_implicates_structure` | inline Turtle, implicates triple included |
| `test_self_deception_same_agent` | `tests/test_deception.py` | converted | `self_deception_same_agent` | inline Turtle, two Participation relators |
| `test_distortion_shacl_passes` | `tests/test_deception.py` | converted | `distortion_shacl_passes` | inline Turtle, spin-doctor participation |
| `test_fabrication_refuted_provenance` | `tests/test_deception.py` | converted | `fabrication_refuted_provenance` | inline Turtle, VerificationResult with failed status |
| `test_forgery_failed_signature_structure` | `tests/test_deception.py` | converted | `forgery_failed_signature_structure` | inline Turtle, CryptographicSignature + counterpartOf |
| `test_impersonation_facet_subject_mismatch` | `tests/test_deception.py` | converted | `impersonation_facet_subject_mismatch` | inline Turtle, IdentityFacet + AuthenticationResult |
| `test_disinformation_propagation_chain` | `tests/test_deception.py` | converted | `disinformation_propagation_chain` | inline Turtle, 3-hop chain, 4 `_doxastic_claim` expansions |
| `test_blame_deflection_example_uses_doxastic_standpoint_claims` | `tests/test_deception.py` | **retained** | — | loads example file from disk and iterates subjects dynamically — no portable Rust equivalent |
| `test_bullshit_modality_exists` | `tests/test_deception.py` | **retained** | — | calls `_graph()` / `load_merged_graph`; cross-slice merged-graph check |
| `test_licensed_falsehood_not_a_lie` | `tests/test_deception.py` | **retained** | — | calls `run_shacl` AND `_graph()` for cross-slice vocabulary assertions (`veridicalityLicensedFalsehood`, `NarrativeReferenceFrame`); the `_graph()` half requires the merged ontology load |
| `test_disinformation_boundary_query` | `tests/test_deception.py` | **retained** | — | uses `load_merged_graph` + external `.rq` competency file + SPARQL SELECT result inspection |

**Tally:** 10 converted, 4 retained. `tests/test_deception.py` 14 → 4 fns.

**Batch 2 parallel migrations** (each `tests/test_<X>.py` → `crates/validate/tests/conformance_<X>.rs`, fixture-only `validate(&nt)` unless noted; inline graphs/helpers reproduced triple-for-triple; assertions preserved):

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_self_private_evidence_triggers_warning` | test_evidence.py | converted | `self_private_evidence_triggers_warning` |
| `test_mixed_evidence_does_not_trigger_self_private_warning` | test_evidence.py | converted | `mixed_evidence_does_not_trigger_self_private_warning` |
| `test_notability_without_triad_triggers_violation` | test_evidence.py | converted | `notability_without_triad_triggers_violation` |
| `test_notability_with_full_triad_passes` | test_evidence.py | converted | `notability_with_full_triad_passes` |
| `test_notability_false_does_not_require_triad` | test_evidence.py | converted | `notability_false_does_not_require_triad` (`_make_citation_act` inlined as `citation_act_ttl()`) |
| `test_infoworld_citation_passes` | test_evidence.py | **retained** | disk fixture load + per-node message check |
| `test_orgbook_citation_passes` | test_evidence.py | **retained** | disk fixture load |
| `test_private_contract_triggers_self_private_warning` | test_evidence.py | **retained** | disk fixture load + per-node warning check |
| `test_orgbook_notability_mutation_triggers_violation` | test_evidence.py | **retained** | dynamic graph mutation (`g.remove`+`g.add`) post-load |
| `test_note_with_content_passes_shacl` | test_notes.py | converted | `note_with_content_passes_shacl` |
| `test_note_with_label_passes_shacl` | test_notes.py | converted | `note_with_label_passes_shacl` |
| `test_note_without_content_or_label_fails_shacl` | test_notes.py | converted | `note_without_content_or_label_fails_shacl` |
| `test_annotation_without_target_fails_shacl` | test_notes.py | converted | `annotation_without_target_fails_shacl` |
| `test_annotation_with_target_passes_shacl` | test_notes.py | converted | `annotation_with_target_passes_shacl` |
| `test_highlight_without_selector_fails_shacl` | test_notes.py | converted | `highlight_without_selector_fails_shacl` |
| `test_highlight_with_selector_passes_shacl` | test_notes.py | converted | `highlight_with_selector_passes_shacl` |
| `test_retracted_note_displayable_false` | test_notes.py | converted | `retracted_note_displayable_false` |
| `test_evidence_span_is_information_object` | test_notes.py | **retained** | cross-slice (EvidenceSpan in evidencespan slice) |
| `test_selector_sub_class_of_evidence_span` | test_notes.py | **retained** | cross-slice (Selector in evidencespan slice) |
| `test_motivation_values_are_individuals` | test_notes.py | **retained** | dynamic count check (`len==10`) |
| `test_notes_are_standpoint_indexed` | test_notes.py | **retained** | cross-slice (`accordingTo` in standpoint slice) |
| `test_notes_*_projection_executable` (×4) | test_notes.py | **retained** | SPARQL parse tests, no SHACL |
| `test_wellformed_participation_conforms` | test_events.py | converted | `wellformed_participation_conforms` (fixture file) |
| `test_malformed_participation_is_flagged` | test_events.py | converted | `malformed_participation_is_flagged` (fixture file) |
| `test_event_is_grounded_in_gufo_event` | test_events.py | **retained** | cross-slice (Activity in provenance slice) |
| `test_former_event_types_are_individuals_not_classes` | test_events.py | **retained** | dynamic subject sweep |
| `test_participation_mediation_axiom_present` | test_events.py | **retained** | BNode `owl:Restriction someValuesFrom` walk |
| `test_contested_event_claims_coexist_and_validate` | test_events.py | **retained** | dynamic multi-file ABox + object sweep |
| `test_schema_*`/`test_ical_*`/`test_owl_time_*`/`test_observational_activity_*` | test_events.py | **retained** | projection stack / cross-slice BNode property-chain |
| spine/expression/manifestation/item/contribution/content_segment SHACL (×8) | test_creative_works.py | converted | `spine_shacl_passes`, `expression_without_work_fails_shacl`, `manifestation_without_expression_fails_shacl`, `item_without_manifestation_fails_shacl`, `contribution_shacl_passes`, `contribution_missing_role_fails_shacl`, `content_segment_shacl_passes`, `content_segment_without_container_fails_shacl` |
| 15 cross-slice/transitive/class-hierarchy tests | test_creative_works.py | **retained** | `_graph()`/`transitive_objects()` over documents/citations/events modules |

**Batch 2 wave 3** (organization, finance, lifecycle, standpoint):

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_membership_fills_post_org_mismatch_warns` | test_organization.py | converted | `membership_fills_post_org_mismatch_warns` (merged mode) |
| `test_legal_identifier_requires_scheme` | test_organization.py | converted | `legal_identifier_requires_scheme` (merged mode) |
| 10 org tests (`test_contested_*`, `test_post_*`, `test_site_location`, `test_change_event_*`, `test_withdrawn_recognition_*`, `test_no_preferred_*`, `test_wellformed_legal_identifier_passes`) | test_organization.py | **retained** | SHACL + `g.objects()`/`g.subjects()` graph-content sweeps, `g.remove()` mutation, or cross-slice `_graph()` |
| `test_finance_fixture_conforms` | test_finance.py | converted | `finance_fixture_conforms` (merged) |
| `test_double_entry_fixture_conforms` | test_finance.py | converted | `double_entry_fixture_conforms` (merged) |
| `test_invoice_fixture_conforms` | test_finance.py | converted | `invoice_fixture_conforms` (merged) |
| `test_order_fixture_conforms` | test_finance.py | converted | `order_fixture_conforms` (merged) |
| `test_holding_fixture_conforms` | test_finance.py | converted | `holding_fixture_conforms` (merged) |
| `test_crypto_fixture_conforms` | test_finance.py | converted | `crypto_fixture_conforms` (merged) |
| 10 finance TBox/absence tests (`test_monetary_*`, `test_currency_*`, `test_no_transaction_subclass_explosion`, `test_*_vocab_is_open_values`, `test_transaction_uses_participation_not_subproperty`) | test_finance.py | **retained** | cross-slice TBox graph-pattern + whole-graph negative sweeps |
| `test_wellformed_entity_existence_conforms` | test_lifecycle.py | converted | `wellformed_entity_existence_conforms` |
| `test_malformed_entity_existence_is_flagged` | test_lifecycle.py | converted | `malformed_entity_existence_is_flagged` |
| 6 lifecycle tests (`test_supersession_*`, `test_lifecycle_event_types_*`, `test_no_lifecycle_event_subclasses_exist`, `test_no_preferred_*`, `test_contested_existence_*`, `test_coverage_fixture_loads_*`) | test_lifecycle.py | **retained** | cross-slice `_graph()` membership + dynamic sweeps + `g.subjects()` |
| `test_coexistence_fixture_conforms` | test_standpoint.py | converted | `coexistence_fixture_conforms` |
| `test_preferred_claim_is_flagged` | test_standpoint.py | converted | `preferred_claim_is_flagged` |
| `test_withdrawn_standpoint_warning_does_not_fail` | test_standpoint.py | converted | `withdrawn_standpoint_warning_does_not_fail` |
| `test_variety_coexistence_fixture_conforms` | test_standpoint.py | converted | `variety_coexistence_fixture_conforms` |
| `test_etymology_coexistence_fixture_conforms` | test_standpoint.py | converted | `etymology_coexistence_fixture_conforms` |
| ~20 standpoint tests (`test_modality_*`, `test_three_axes_*`, `test_vantage_*`, `test_according_to_*`, `test_*_projection_*`, `test_*_maps_to_*`, statement-DSL + SSSOM-mapping) | test_standpoint.py | **retained** | dynamic `_graph()` sweeps / OWL-restriction walks / SPARQL competency / SSSOM mapping / statement-DSL disk |

**Batch 2 wave-3 tally:** organization 2 + finance 6 + lifecycle 2 + standpoint 5 = **15 converted**.

**Batch 2 wave 4** (rights, registers, profiles, privacy — all fixture-backed `validate(&nt)`):

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_wellformed_rights_fixture_conforms` | test_rights.py | converted | `wellformed_rights_fixture_conforms` |
| `test_malformed_rights_fixture_is_flagged` | test_rights.py | converted | `malformed_rights_fixture_is_flagged` |
| `test_expired_trademark_warns_but_does_not_fail` | test_rights.py | converted | `expired_trademark_warns_but_does_not_fail` (warning-tolerant) |
| 7 rights tests (`test_expanded_action_vocabulary_is_seeded`, 6 `test_*_projection_emits_*`) | test_rights.py | **retained** | dynamic subjects sweep / projection + `(triple) in out` membership |
| `test_wellformed_registers_fixture_conforms` | test_registers.py | converted | `wellformed_registers_fixture_conforms` |
| `test_malformed_registers_fixture_is_flagged` | test_registers.py | converted | `malformed_registers_fixture_is_flagged` (4 violation substrings) |
| 7 registers tests (`test_register_spine_*`, `test_persona_*`, `test_expression_machinery_*`, `test_style_guide_*`, `test_no_primary_persona_*`, `test_same_norms_invariant_*`, `test_divergence_query_*`) | test_registers.py | **retained** | `_graph()` TBox checks / dynamic subject sweep / SPARQL competency |
| `test_profile_shape_passes_for_wellformed_profile` | test_profiles.py | converted | `profile_shape_passes_for_wellformed_profile` |
| `test_profile_shape_fails_for_invalid_profile_applies_to` | test_profiles.py | converted | `profile_shape_fails_for_invalid_profile_applies_to` |
| `test_profile_open_value_guard_warns_on_orphan` | test_profiles.py | converted | `profile_open_value_guard_warns_on_orphan` (warning-tolerant) |
| `test_wellformed_privacy_fixture_conforms` | test_privacy.py | converted | `wellformed_privacy_fixture_conforms` |
| `test_malformed_privacy_fixture_is_flagged` | test_privacy.py | converted | `malformed_privacy_fixture_is_flagged` |
| `test_sensitive_value_warns_but_does_not_fail` | test_privacy.py | converted | `sensitive_value_warns_but_does_not_fail` (warning-tolerant) |
| 11 privacy tests | test_privacy.py | **retained** | `_graph()` TBox membership / `load_merged_graph` subject iteration / projection |

**Batch 2 wave-4 tally:** rights 3 + registers 2 + profiles 3 + privacy 3 = **11 converted**.

**Batch 2 wave 5** (teleology, norms, myth, narrative):

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_wellformed_teleology_fixture_conforms` | test_teleology.py | converted | `wellformed_teleology_fixture_conforms` |
| `test_malformed_teleology_fixture_is_flagged` | test_teleology.py | converted | `malformed_teleology_fixture_is_flagged` |
| 3 teleology tests | test_teleology.py | **retained** | cross-slice `(triple) in g` membership / dynamic `g.subjects()` sweep / SPARQL `.rq` |
| `test_wellformed_norms_fixture_conforms` | test_norms.py | converted | `wellformed_norms_fixture_conforms` (fixture-only — `validate` not `validate_with_ontology`, to avoid the WEMI-embodies inference the merged base adds) |
| `test_malformed_norms_fixture_is_flagged` | test_norms.py | converted | `malformed_norms_fixture_is_flagged` |
| 4 norms tests (`test_graft_*`, `test_competency_*`) | test_norms.py | **retained** | cross-slice file-load + `(triple) in g` / SPARQL `.rq` |
| `test_myth_shacl_passes` | test_myth.py | converted | `myth_shacl_passes` (`_add_narrative_frame` inlined as `narrative_frame_ttl`) |
| `test_myth_missing_frame_fails_shacl` | test_myth.py | converted | `myth_missing_frame_fails_shacl` |
| `test_myth_propagation_shacl_passes` | test_myth.py | converted | `myth_propagation_shacl_passes` |
| 10 myth tests | test_myth.py | **retained** | `_graph()` TBox membership / dynamic sweeps / BNode OWL-restriction walk |
| `test_narrative_reference_frame_shacl_passes` | test_narrative.py | converted | `narrative_reference_frame_shacl_passes` |
| `test_narrative_frame_link_shacl_passes` | test_narrative.py | converted | `narrative_frame_link_shacl_passes` |
| `test_character_arc_shacl_passes` | test_narrative.py | converted | `character_arc_shacl_passes` |
| `test_character_arc_missing_subject_fails_shacl` | test_narrative.py | converted | `character_arc_missing_subject_fails_shacl` |
| 4 narrative tests | test_narrative.py | **retained** | transitive `subClassOf` walk / cross-slice `(triple) in g` (documents/places modules) |

**Batch 2 wave-5 tally:** teleology 2 + norms 2 + myth 3 + narrative 4 = **11 converted**.

**Batch 2 grand tally:** deception 10 + evidence 5 + notes 8 + events 2 + creative_works 8 + organization 2 + finance 6 + lifecycle 2 + standpoint 5 + rights 3 + registers 2 + profiles 3 + privacy 3 + teleology 2 + norms 2 + myth 3 + narrative 4 = **70 converted** across 17 slices; retained (cross-slice/dynamic/SPARQL/disk-load/SSSOM/projection) tracked per-fn above. All **87** conformance tests (17 batch-1 + 70 batch-2) green; `uv run mypy` clean (281 files).

### #867 conformance batch 3 (citations, cognition, reference_frames, narration, aboutness, disclosure)

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_citation_act_shacl_passes` | test_citations.py | converted | `citation_act_shacl_passes` |
| `test_citation_act_missing_intent_fails_shacl` | test_citations.py | converted | `citation_act_missing_intent_fails_shacl` |
| `test_contribution_with_degree_shacl_passes` | test_citations.py | converted | `contribution_with_degree_shacl_passes` |
| 3 citations tests | test_citations.py | **retained** | self-description loader / disk-TTL `(triple) in g` / cross-format sweep |
| `test_wellformed_knowledge_proficiency_conforms` | test_cognition.py | converted | `wellformed_knowledge_proficiency_conforms` (merged) |
| `test_malformed_knowledge_proficiency_is_flagged` | test_cognition.py | converted | `malformed_knowledge_proficiency_is_flagged` (merged) |
| ~6 cognition tests | test_cognition.py | **retained** | `_graph()` TBox / dynamic metaclass sweep / SSSOM mapping |
| 8 reference-frame realm tests (measurement/currency/temporal/colourspace/linguistic/mathematical/narrative/biological) | test_reference_frames.py | converted | `*_reference_frame_passes` etc. (all 8, fixture-only) |
| `test_wellformed_narration_fixture_conforms` | test_narration.py | converted | `wellformed_narration_fixture_conforms` |
| `test_malformed_narration_fixture_is_flagged` | test_narration.py | converted | `malformed_narration_fixture_is_flagged` |
| 7 narration tests | test_narration.py | **retained** | `_graph()` TBox/subject-iteration / efficiency-budget quad scan / SPARQL |
| `test_wellformed_aboutness_fixture_conforms` | test_aboutness.py | converted | `wellformed_aboutness_fixture_conforms` |
| `test_malformed_aboutness_fixture_is_flagged` | test_aboutness.py | converted | `malformed_aboutness_fixture_is_flagged` |
| 7 aboutness tests | test_aboutness.py | **retained** | `_graph()` TBox / dynamic subject+combinations sweeps / SPARQL / labeled-defined sweep |
| `test_leak_fixture_is_flagged` | test_disclosure.py | converted | `leak_fixture_is_flagged` |
| `test_wellformed_disclosure_fixture_conforms` | test_disclosure.py | converted | `wellformed_disclosure_fixture_conforms` |
| `test_conditional_disclosure_warns_but_does_not_fail` | test_disclosure.py | converted | `conditional_disclosure_warns_but_does_not_fail` (warning-tolerant) |
| ~12 disclosure tests | test_disclosure.py | **retained** | `_graph()` TBox / dynamic sweeps / `.rq` SPARQL / disk-iterate |

**Conformance batch-3 (wave 1+2) tally:** citations 3 + cognition 2 + reference_frames 8 + narration 2 + aboutness 2 + disclosure 3 = **20 converted**. genealogy + employment had ZERO migratable tests (every run_shacl call is paired with a dynamic `g.objects()`/`(triple) in g`/post-SHACL membership assertion — retained whole, files unchanged). All **107** conformance tests (87 prior + 20) green; `uv run mypy` clean.

### #867 conformance batch 3 wave 3 (risk, interior, narrative_time, software) + faithfulness fixes

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_wellformed_risk_fixture_conforms` | test_risk.py | converted | `wellformed_risk_fixture_conforms` |
| `test_malformed_risk_fixture_is_flagged` | test_risk.py | converted | `malformed_risk_fixture_is_flagged` |
| 2 risk tests | test_risk.py | **retained** | dynamic `g.subjects()` occurrence-gate sweep / SPARQL `.rq` |
| `test_wellformed_interior_fixture_conforms` | test_interior.py | converted | `wellformed_interior_fixture_conforms` |
| `test_malformed_interior_fixture_is_flagged` | test_interior.py | converted | `malformed_interior_fixture_is_flagged` |
| 10 interior tests | test_interior.py | **retained** | `_graph()` TBox membership / dynamic sweeps / SPARQL |
| `test_wellformed_narrative_time_fixture_conforms` | test_narrative_time.py | converted | `wellformed_narrative_time_fixture_conforms` |
| `test_malformed_narrative_time_fixture_is_flagged` | test_narrative_time.py | converted | `malformed_narrative_time_fixture_is_flagged` |
| 7 narrative_time tests | test_narrative_time.py | **retained** | `_graph()` TBox / dynamic sweep / disk graph-walk / SPARQL |
| `test_facet_orthogonality_shacl_rejects_two_facets` | test_software.py | converted | `facet_orthogonality_shacl_rejects_two_facets` |
| `test_fixture_parses_and_shacl_passes` | test_software.py | converted | `fixture_parses_and_shacl_passes` |
| 13 software tests | test_software.py | **retained** | `(triple) in g` membership / `_graph()` + combinations / dynamic sweeps |

**FAITHFULNESS FIXES (mode correction):** four conformance twins were authored with `validate_with_ontology` (merged) but their Python originals used fixture-only `run_shacl(_fixture(...))` / `run_shacl(Graph().parse(...))`. Corrected to fixture-only `validate()` to match exactly: `conformance_narrative_time.rs` (×2, also REVERTED two `tests/fixtures/shapes/narrative-time-*.ttl` files an agent had edited to force a merged-mode pass — a fixture fudge), `conformance_software.rs` (×1), `conformance_cognition.rs` (×2, this branch), `conformance_organization.rs` (×2, correcting a latent drift merged in #961 — `Graph().parse(coverage/organization-*.ttl)` is fixture-only). Only `conformance_finance.rs` legitimately uses `validate_with_ontology` (Python `g = _graph()`). All pass fixture-only with original fixtures.

**Conformance batch-3 wave-3 tally:** risk 2 + interior 2 + narrative_time 2 + software 2 = **8 converted**. All **115** conformance tests (107 prior + 8) green; `uv run mypy` clean.

### #867 conformance batch 3 wave 4 (trust, rubrics, names, music_analysis)

| pytest fn | source | disposition | Rust twin / retain reason |
|-----------|--------|------------|---------------------------|
| `test_contested_certification_coexists` | test_trust.py | converted | `contested_certification_coexists` (fixture-only — `Graph().parse(coverage/trust-contested.ttl)`) |
| 2 trust tests | test_trust.py | **retained** | `_graph()` orthogonality + dynamic banned-term sweep |
| `test_wellformed_rubrics_fixture_conforms` | test_rubrics.py | converted | `wellformed_rubrics_fixture_conforms` (fixture-only) |
| `test_malformed_rubrics_fixture_is_flagged` | test_rubrics.py | converted | `malformed_rubrics_fixture_is_flagged` (12 violation substrings) |
| 7 rubrics tests | test_rubrics.py | **retained** | `_graph()` TBox / `g.subjects()` judge-walk / SPARQL |
| 8 names tests | test_names.py | **retained** | ALL: defined-class Collection traversal / dynamic pronoun-form sweep / `g.objects()` membership / cross-slice `_graph()` (Appellation subclasses, hasTitle, hasSoftwareName home in other slices) — ZERO migratable |
| `test_music_analysis_claim_shape_passes` | test_music_analysis.py | converted | `music_analysis_claim_shape_passes` (**merged** — Python `g = _graph()`) |
| `test_music_analysis_claim_missing_frame_fails` | test_music_analysis.py | converted | `music_analysis_claim_missing_frame_fails` (**merged** — Python `g = _graph()`) |
| `test_genre_no_subclass_shape_fails_on_bad_subclass` | test_music_analysis.py | converted | `genre_no_subclass_shape_fails_on_bad_subclass` (fixture-only) |
| 9 music_analysis tests | test_music_analysis.py | **retained** | TBox membership / dynamic genre+frame sweeps / DSL-compile |

**Conformance batch-3 wave-4 tally:** trust 1 + rubrics 2 + names 0 + music_analysis 3 = **6 converted**. All **121** conformance tests (115 prior + 6) green; `uv run mypy` clean.

**Conformance batch-3 GRAND tally:** citations 3 + cognition 2 + reference_frames 8 + narration 2 + aboutness 2 + disclosure 3 + risk 2 + interior 2 + narrative_time 2 + software 2 + trust 1 + rubrics 2 + music_analysis 3 = **34 converted** across 13 slices (+ 4 faithfulness mode-fixes). genealogy/employment/names had ZERO migratable. Total native conformance suite = **121 tests** (#957: 17, #961: 70, batch-3: 34). Remaining run_shacl files: places 34, images 12, music_collections 10, music_pitch 9, + 1-count whole-graph validations (agentic/ai_claims/up_projection/identity_over_history/foundation_import/verifiable_release_chain).

### #867 conformance batch 4 (the big files: places, images, music_collections, music_pitch)

All migrated twins use fixture-only `validate()` (every Python original is `run_shacl(_fixture(...))` / `run_shacl(Graph())`); no `validate_with_ontology`, no fixture edits.

| source | converted | Rust twins | retained |
|--------|-----------|-----------|----------|
| test_places.py | 3 | `biological_coverage_passes_shacl`, `geocode_shape_valid`, `cadastral_coverage_passes_shacl` | ~39 (bnode owl:unionOf walks, RCC-8 JEPD disjoint sweeps, `g.subjects()`/`g.objects()` post-SHACL membership, decimal-ABox numeric, cross-slice `_graph()`) |
| test_images.py | 11 | depiction_usage (pass/missing-image/multi-subject), image_region (pass/missing-selector), scene_graph_edge (pass/missing-relation/confidence-range), region_selector_missing_value, media_object_colourspace (pass/missing-warns) | 7 (`_graph()` TBox + colourspace cross-slice) |
| test_music_collections.py | 11 | pitch_collection (kind/membership ×4), pitch_spelling (×4), standpoint_memberships arabic+turkish | 15 (`_graph()` TBox/`(triple) in g`, functional-property, seed/coexist) |
| test_music_pitch.py | 9 | pitch_value (ratio/cents/missing-frame/both-fails/zero-denom), pitch_interval_xor (×3), tuning_system_requires_kind_and_realm | 10 (`_graph()` TBox, dynamic property sweeps, seed coexist) |

**Conformance batch-4 tally:** places 3 + images 11 + music_collections 11 + music_pitch 9 = **34 converted**. All **155** conformance tests (121 prior + 34) green.

### #867 conformance batch 4 — whole-graph / single-run_shacl tail

| source | converted | Rust twin / retain reason |
|--------|-----------|---------------------------|
| test_ai_claims.py | 1 | `normative_fixture_validates_against_the_full_graph` (**merged** — Python `load_merged_graph()+ai-normative.ttl`); 11 retained (tombstone-absence + TBox) |
| test_agentic.py | 1 | `double_valued_toolcall_violates_the_closed_world_twins` (**merged** — Python `run_shacl(_graph()+data)`, asserts violation substrings); 4 retained (SPARQL/memory/MCP) |
| test_verifiable_release_chain.py | 1 | `fixture_loads_and_shacl_passes` (fixture-only — `Graph().parse(verifiable-release-chain.ttl)`); 16 retained (chain `(triple) in g` membership / SPARQL / signing) |
| test_up_projection.py | 0 | **all retained** — both run_shacl callers use a CUSTOM `STATEMENT_DSL_SHAPES_FILE` shapes path; the conformance harness always uses the full merged corpus, so custom-shapes validations are NOT reproducible (faithful retain) |
| test_foundation_import.py | 0 | **all retained** — the sole run_shacl validates a dynamic disk-corpus importer pipeline (`FoundationImporter.import_corpus`), not a static fixture |
| test_identity_over_history.py | 0 | **all retained** — the sole run_shacl is paired with post-SHACL `(triple) in g` membership checks on the same graph; non-separable |

**Conformance batch-4 GRAND tally:** places 3 + images 11 + music_collections 11 + music_pitch 9 + ai_claims 1 + agentic 1 + verifiable_release_chain 1 = **37 converted**. Total native conformance suite = **158 tests**. The 4 `validate_with_ontology` (merged) users — finance, music_analysis, ai_claims, agentic — are all faithful (Python `g = _graph()`/`load_merged_graph()`). The run_shacl pytest population is now exhausted: every remaining run_shacl call is a faithful RETAIN (custom-shapes, dynamic disk/sweep, post-SHACL graph-membership, or cross-slice `_graph()` TBox) documented per-file above. `uv run mypy` clean (281 files).

## #867 structural batch 13 (author-fresh: entities / sources / language / pipeline / graphrag)

Five further slices that the batch-12 triage had pencilled in as "exempt" but which, on
re-verification (the `verify-descope-claims` discipline — confirm before documenting), carry
**genuine assertable asserted-TBox** worth pinning. Per `.goals` MAXIMAL coverage / NO
COMPROMISES, a slice that *can* hold a faithful structural cell gets one rather than an exemption
row. None had migratable rdflib pytest (their pytest, where any, is projection/ABox/behavior — KEEP),
so these are net-new declarative coverage; no pytest deleted. All green
(`cargo nextest -p gmeow-slicetest`); every `mustNot` cell red-proofed (2026-06-25).

| Slice | Cells (must / mustNot) | Keystone invariants pinned |
|---|---|---|
| `core/entities` | 4 (3 / 1) | agent-Kind spine (Person/Organization/SoftwareAgent ⊑ Agent; Group ⊑ Collection+Entity); entity property shapes; Person/Organization/SoftwareAgent in an AllDisjointClasses; Group deliberately NOT disjoint with Organization |
| `core/sources` | 3 (1 / 2) | contentDigest/sourceLocation/sourceModifiedAt property shapes; contentDigest domain-free; none functional (digests/copies coexist) — complements the existing competency.ttl |
| `core/language` | 4 (3 / 1) | Language/WritingSystem Kinds ⊑ InformationObject + Formal/Programming SubKind spine; TransliterationScheme vocab + bcp47Tag/writtenInLanguage/transliterationScheme; en/fr/zh seeds; every Language-refinement carries logic:SubKind |
| `core/pipeline` | 4 (3 / 1) | Pipeline/PipelineStage Kinds ⊑ SocialObject + StageKind vocab; dataflow/stage property shapes; 7 StageKind seeds; every dogfooded PipelineStage carries a stageKind (build-pipeline integrity) |
| `extensions/graphrag` | 4 (3 / 1) | Corpus/Community/ExtractedEntity/Embedding/VectorIndex Kinds ⊑ InformationObject + RetrievalEvent ⊑ Activity; DistanceMetric/IndexAlgorithm vocabs + seeds; functional retrieval edges; value vocabs never subclassed |

**#869 Gap-1:** the three VALUES `mustNot` patterns (sources not-functional, graphrag
vocab-not-subclassed; and the language/entities/pipeline FILTER-NOT-EXISTS guards) range over the
**fixed term sets each slice declares** (its three carrier properties, its two value-vocab classes)
or use a dynamic `FILTER NOT EXISTS` over an open population (every Language-refinement class, every
PipelineStage individual) — never a finite blacklist standing in for an open universal.

Both the inference `saInferenceCommitmentNoSubclasses` and the graphrag `saGraphRagVocabsNotSubclassed` `mustNot` cells carry a `FILTER (?c != self)` reflexive self-exclusion guard, matching the idiom already present in language/kernel/dreaming; the guard is RDFS-reasoner-safe and is currently a no-op in the asserted graph, but future-proofs against a `saReasoning` materialization of rdfs10.

**Batch-13 tally:** 19 fresh structural cells across 5 slices (13 must + 6 mustNot); 0 pytest deleted.

## #867 structural Tier-B CLOSEOUT — all 76 slices accounted

With batches 11–13, every one of the 12 slices that previously lacked `tests/structural.ttl` is
accounted for. **10 now carry a structural cell** (kernel migrated from pytest; accounts, guides,
inference, dreaming, entities, sources, language, pipeline, graphrag authored fresh). **2 are
documented exemptions** (re-verified against the live slice, `verify-descope-claims`):

| Exempt slice | Reason (verified 2026-06-25) |
|---|---|
| `core/inhabitation` | **No `module.ttl`** — design-only placeholder (`slices/core/inhabitation/design/`, branch `paudley/inhabitation-design`); there is no asserted TBox to assert over. A structural cell follows when the module lands. |
| `core/logic` | **Principle 17** — `logic:` is the canonical reasoning vocabulary; its structural invariants are enforced by the Rust foundation oracle (`crates/foundation`, `foundation.rs`) and the gufo→logic migration validators, NOT by SHACL/SPARQL projections. A `structural.ttl` over `logic/module.ttl` would duplicate the Rust authority and risk divergence; its pytest (`test_logic_*.py`) exercises the solver ENGINE (KEEP), not TBox shape. |

**Structural Tier-B is now 76/76 accounted: 74 slices carry `structural.ttl` + 2 documented
exemptions, nothing silently dropped.** (The remaining open #867 work is Tier-C cull-on-retirement,
gated on #832 / Python-oracle retirement — unchanged by this parcel.)

## #867 structural batch 12 (author-fresh: accounts / guides / inference / dreaming)

Four slices with real asserted-TBox vocabulary but **no migratable rdflib pytest** (they never
had a `tests/test_<slice>.py` of structural assertions). Rather than leave them uncovered, fresh
`structural.ttl` cells pin their keystone invariants — MAXIMAL coverage + dogfooding (`.goals`).
No pytest is deleted (there was none to delete); these are net-new declarative coverage. All green
(`cargo nextest -p gmeow-slicetest`); every `mustNot` cell red-proofed (2026-06-25).

| Slice | Cells (must / mustNot) | Keystone invariants pinned |
|---|---|---|
| `extensions/dreaming` | 4 (2 / 2) | DreamReport = composed recollection Experience + the 2 hasValue restrictions; dreamElement domain-Experience object property + open range; no-new-named-class (pure composition, Principle 4/6) |
| `core/accounts` | 5 (4 / 1) | AccountStatus/ServiceStatus QualityValue vocabs (OPEN — seeds present, NOT closed); OnlineAccount/OnlineService InformationObject Kinds; accountService/serviceShutdownDate functional shapes; accountStatus/serviceStatus NOT functional (Principle 9, prevents sameAs collapse) |
| `core/guides` | 5 (2 / 3) | Recipe/LearningPath QualityValue vocabs; includesRecipe LearningPath→Recipe shape; every dogfooded Recipe well-formed (slug+title+goal); every LearningPath has audience+goal; includesRecipe referential integrity (targets are typed Recipes) |
| `core/inference` | 7 (5 / 2) | Analogy/Correspondence/InferenceCommitment = Relator Kinds; Process/Tenure endurant-occurrent spine; InferenceMode/DefeaterKind value vocabs + Peirce tetrad + Pollock seeds; functional argument edges (conclusion/inferenceModeOf/correspondingSource-Target/tenureOf); competesWith symmetric; NO subclass of InferenceCommitment (Principle 9 — modes are values); open-range (Principle 13) argument inputs |

**#869 Gap-1:** the two `mustNot` VALUES patterns (accounts status-properties, inference open-range
inputs) enumerate the **fixed, closed set of properties each slice declares**, not a stand-in for
an open universal. The two OPEN value vocabularies (AccountStatus, ServiceStatus) and the
"closed-but-open" InferenceMode/DefeaterKind vocabularies are asserted present-but-not-closed,
faithfully matching their module doctrine.

**Batch-12 tally:** 21 fresh structural cells across 4 slices (13 must + 8 mustNot); 0 pytest
deleted (none existed); net-new coverage. With kernel (batch 11) + these four, the 5 author/migrate
slices of the #867 structural closeout are done.

## #867 structural batch 11 (kernel — the universal aboutness axis)

`tests/test_aboutness.py` (the kernel-resident #349/EPIC-#348 aboutness axis, 7 fns) migrated
to `slices/core/kernel/tests/{structural,competency}.ttl`. 3 structural fns → 5
`gmeow:StructuralAssertion` cells (3 must + 2 mustNot) + 1 competency fn → 1
`gmeow:CompetencyQuestion` cell; all green (`cargo nextest -p gmeow-slicetest`). `make validate` ✓.
Per-cell `saRationale`/`cqRationale` names the source fn. kernel is #694-migrated (logic:).

| Source fn | Disposition |
|---|---|
| `test_aboutness_class_structure` | → `saAboutnessModeClass` (must): AboutnessMode a owl:Class, logic:AbstractIndividualType; subClassOf logic:QualityValue |
| `test_has_aboutness_property_structure` | → `saHasAboutnessAnnotationProperty` (must: AnnotationProperty + range) + `saHasAboutnessNotObjectFunctionalOrDomained` (mustNot: ObjectProperty/FunctionalProperty/domain) |
| `test_value_vocab_spans_two_seeds` | → `saAboutnessSeedsTyped` (must: both seeds typed) + `saAboutnessNoThirdMember` (mustNot: closed two-member enumeration, kernel-scoped) |
| `test_competency_aboutness_modes_query` | → `cqAboutnessModes` (competency, cqExactRows over `queries/competency/aboutness-modes.rq`) |

**Retained (2 pytest fns) — merged-graph absence, not module-expressible:**

- `test_aboutness_orthogonal_to_other_axes` — asserts NO `rdfs:subPropertyOf`/`owl:equivalentProperty`
  between any pair of the 6 epistemic axes (`hasAboutness`, `hasGranularity`, `hasDeterminacy`,
  `hasSensitivity`, `hasDisclosurePolicy`, `confidence`). Those axes are declared/used across
  10+ slice modules, so the absence must hold over the whole merged graph
  (`load_merged_graph(include_imports=False)`); a `gmeow:scopeModule` cell sees only kernel and
  would silently weaken the cross-axis universal.
- `test_no_aboutness_truth_bridge` — asserts the seeds carry exactly `{AboutnessMode}` as type.
  `gmeow:aboutnessEnacts` is referenced from `core/citations` and `extensions/norms`, so the
  exactly-one-type guarantee is a merged-graph property, not a kernel-module one.

**#869 Gap-1:** `saAboutnessNoThirdMember` is a closed two-member enumeration the source already
pinned (`members == {describes, enacts}`) — a genuinely-closed value vocabulary, not a finite-VALUES
blacklist standing in for an open universal. The cross-module "no OTHER slice adds a member"
guarantee is carried by the retained merged-graph pytest, not weakened away.

**Red-proof (2026-06-25).** Injecting a transient `gmeow:aboutnessBOGUS a gmeow:AboutnessMode`
into `slices/core/kernel/module.ttl` reds `saAboutnessNoThirdMember` (`polarity 'mustNot' but the
ASK pattern HELD`); reverted, green again. Cells authored by MEASURING the live kernel module.

**Batch-11 tally:** 3 converted structural fns → 5 cells + 1 competency fn → 1 cell; 2
retained-with-reason; 1 deleted (`test_every_term_labeled_and_defined` — the per-term
label+definition sweep is subsumed by `make validate`'s `Gmeow*Shape` SHACL + the
`structural_lint` annotation sweep; break-and-revert confirmed `make validate` reds on a dropped
label). `tests/test_aboutness.py` is trimmed to the 2 retained fns + the `_graph()` helper.

## #867 structural batch 10 (places — the 129-fn slice)

The largest single slice (129 pytest fns) migrated to `slices/core/places/tests/structural.ttl`
in its own parcel (the cell file is ~1750 lines, authored via chunked writes). ~87 converted
fns → 122 `gmeow:StructuralAssertion` cells (94 must + 28 mustNot); all green
(`cargo nextest -p gmeow-slicetest`, 61/61 cell files). places is #694-migrated (logic:).
`make validate` ✓. The OWL-RL chain/location-propagation tests were already migrated to Rust
(#896) and are NOT re-introduced.

**Retained (42 pytest fns):**

- **35 run_shacl ExampleConformance** — coexistence/contested/superseded/lapsed-tenure ABox
  fixtures, geocode/cadastral/biological shape validation, Decimal numeric checks.
- **~5 dynamic / bnode** — `owl:unionOf` list walks, `owl:propertyChainAxiom` bnode checks,
  `owl:AllDisjointProperties` sweeps, a label string-content check.
- **3 cross-slice** (restored from cells the harness rejected): `test_postal_address_frame_property`
  and `test_address_components_present_and_nonfunctional` (postalAddressFrame/addressPlace/
  streetAddress home-asserted in `core/contacts`); `test_alternate_name_retired` (the
  `gmeow:hasPlaceName rdfs:range gmeow:PlaceName` arm is cross-slice — hasPlaceName/PlaceName
  in `core/names`).

**Batch-10 tally:** ~87 converted fns → 122 structural cells; 42 retained-with-reason. The
4 harness-rejected cross-slice cells (postal-address + hasPlaceName range) were removed and
their 3 source fns restored to pytest — the harness's module-locality check caught them, not
an assumption. With places done, the structural Tier-B migration covers every slice that has
migratable asserted-TBox pytest (57 slices carry `structural.ttl`); the only un-migrated
pytest left is run_shacl ExampleConformance (a separate `example-conformance.ttl` cell type),
competency questions (`competency.ttl`), permanent Keeps (CLI/MCP/generator/numeric/dynamic
sweeps/SSSOM-mapping reads), and the Tier-C parity tests gated on the oracle/purrdf/native-
producer retirements.

## #867 structural batch 9 (creative-works / genealogy)

Two more home slices migrated to `slices/<g>/<n>/tests/structural.ttl` (both gufo:).
20 converted fns → 33 `gmeow:StructuralAssertion` cells; all green
(`cargo nextest -p gmeow-slicetest`, 60/60 cell files). `make validate` ✓. Per-cell
`saRationale` names the source fn. (`places` — the 129-fn slice — is deferred: its
structural.ttl is too large to author in a single write; tracked for a follow-on parcel.
`music` has no migratable structural fns, all-retained, assessed in batch 2.)

| Slice (stereotype) | Converted fns → cells | Retained fns (reason) |
|---|---|---|
| `core/creative-works` (gufo:) | 17 → 27 cells | 22 retained: cross-slice WEMI subjects (CreativeWork/Document/Article/MediaObject/BookRelease + ContributionDegree + eventType* defined in documents/citations/events); `transitive_objects` dynamic traversal; 6 run_shacl; the #156 book/narrative tests (subjects in documents); 8 run_shacl ExampleConformance |
| `extensions/genealogy` (gufo:) | 3 → 6 cells | 5 retained: 2 whole-graph dynamic sweeps (no-former-event-subclass, no-preferred); 3 run_shacl ExampleConformance. genealogy OWNS the 3 KinRelationship bridges (relationshipParent/relationshipChild/hasPartner subPropertyOf observedFeature) that the observations slice could not see (#867 batch-1 cross-slice deferral) — now migrated home as cells |

**Batch-9 tally:** 20 converted fns → 33 structural cells across 2 slices; 27
retained-with-reason. Notably closes the cross-slice loop from batch 1: the 3
KinRelationship observation-bridges, retained-in-pytest there as "pending a genealogy
migration", are now genealogy `structural.ttl` cells.

## #867 structural batch 8 (attestation / standpoint / archaeological-evidence / notes / sensory / sensory-environment)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl`. ~83 converted
fns → 125 `gmeow:StructuralAssertion` cells; all green (`cargo nextest -p gmeow-slicetest`,
70/70 cell files). `standpoint` is #694-migrated (logic:); the rest gufo:. sensory +
sensory-environment had their OWL-RL reasoning tests migrated to Rust in #896 — this
batch migrates their remaining ASSERTED-TBox structural fns (the SOSA/AFO `*_mapped_to_*`
mapping reads stay retained). Per-cell `saRationale` names the source fn. Every RETAINED
fn is rowed below (no silent drops). `make validate` ✓.

| Slice (stereotype) | Converted fns → cells | Retained fns (reason) |
|---|---|---|
| `core/attestation` (gufo:) | 14 → 24 cells | `test_certification_still_exists_as_relator` (cross-slice: Certification in trust); 2 run_shacl ExampleConformance |
| `core/standpoint` (logic:) | 5 → 14 cells | 32 retained: foundational slice with heavy cross-slice (wasAttributedTo/confidence in provenance, containedInPlace/Place in places); 5 run_shacl; 5 SSSOM mapping reads; whole-graph dynamic sweeps (modality `==` over merged graph, no-preferred); bnode tenure-restriction walk |
| `extensions/archaeological-evidence` (gufo:) | 29 → 42 cells | `test_attested_on_carrier_exists` (cross-slice: attestedOnCarrier in lexicon); `test_no_primary_or_preferred_archaeological_terms` (whole-graph sweep) |
| `extensions/notes` (gufo:) | 16 → 23 cells | 16 retained: 3 cross-slice (EvidenceSpan/Selector in evidence, accordingTo in standpoint); `motivation` numeric-count gate (seeds+ban migrated); 7 run_shacl + 1 retracted; 4 projection parse tests |
| `extensions/sensory` (gufo:) | 7 → 7 cells | 7 retained: all SOSA/AFO `*_mapped_to_*` + AFO-equivalences reads (generated mapping artifacts) — the reasoning tests were already migrated to Rust (#896) |
| `extensions/sensory-environment` (gufo:) | 12 → 15 cells | 5 retained: 2 SSSOM mapping reads; 2 cross-slice (axis/frame-realm in places); the consistency arm of the mixed `mental_reference_frame_requires_host` (its structural restriction arm → cell) |

**Batch-8 tally:** ~83 converted fns → 125 structural cells across 6 slices; ~65
retained-with-reason (cross-slice subjects, run_shacl ExampleConformance, SSSOM-mapping
reads, whole-graph dynamic sweeps, bnode list-walks, projection-parse + numeric gates).
sensory/sensory-environment complete the #896 reasoning-cluster slices' structural tails.

## #867 structural batch 7 (names / events / software / finance / images / cognition)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl`. ~94 converted
fns → 179 `gmeow:StructuralAssertion` cells; all green (`cargo nextest -p gmeow-slicetest`,
64/64 cell files). `names` and `events` are #694-migrated (logic:); the rest gufo:.
Per-cell `saRationale` names the source fn; detailed cell IRIs live in each
`structural.ttl`. Every RETAINED fn is rowed below (no silent drops). `make validate` ✓.

| Slice (stereotype) | Converted fns → cells | Retained fns (reason) |
|---|---|---|
| `core/names` (logic:) | 20 → 44 cells | 8 retained: `test_appellation_umbrella_and_structural_subclasses` + `test_has_title_subproperty_of_hasappellation` + `test_has_software_name_subproperty_of_hasappellation` (CROSS-SLICE — the Appellation subclasses / hasAppellation bridges are asserted in organization/creative-works/agreements/software/documents, not names — caught by the harness, restored to pytest); `test_place_naming_is_defined_class` (partial: equivalentClass Collection traversal); 21-item dynamic pronoun-anchor sweep; pronoun name-only ABox; `test_contested_name_usage_coexists` (run_shacl); audience/standpoint cross-slice |
| `core/events` (logic:) | 24 → 55 cells | 16 retained: cross-slice (Activity⊑Event in provenance, observational-activity); dynamic `g.subjects(subClassOf)` sweeps; bnode mediation/property-chain list walks; 2 run_shacl + contested fixture; 8 projection tests |
| `extensions/software` (gufo:) | 11 → 21 cells | 17 retained: dynamic pairwise facet sweep; run_shacl + 13 fixture-ABox checks; 2 dynamic subset sweeps |
| `extensions/finance` (gufo:) | ~17 → 28 cells | 16 retained: cross-slice (MonetaryAmount/currency/reference-frame in core); 6 run_shacl; whole-graph absence sweeps (no-subclass-explosion + the negative arms of the value-vocab partials) |
| `extensions/images` (gufo:) | 10 → 11 cells | 18 retained: 7 cross-slice (depicts/MediaObject/image-event-types in core/documents + core/events); 11 run_shacl ExampleConformance |
| `core/cognition` (gufo:) | 13 → 20 cells | 9 retained: cross-slice (MentalMoment in kernel, IntentionalMode in teleology, proficiency-vocab in kernel); dynamic metaclass-cardinality; 2 run_shacl; 3 SSSOM mapping reads |

**Batch-7 tally:** ~94 converted fns → 179 structural cells across 6 slices (the
largest batch); ~84 retained-with-reason (cross-slice subjects, run_shacl
ExampleConformance, whole-graph dynamic sweeps, bnode list-walks, projection +
SSSOM-mapping + numeric reads). names' 3 cross-slice Appellation-bridge cells were
caught by the harness (must-cell failed) and correctly moved back to retained pytest.

## #867 structural batch 6 (coreference / organization / agentic / evidence / narrative / deception)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl` (all gufo:).
50 converted fns → 108 `gmeow:StructuralAssertion` cells; all green
(`cargo nextest -p gmeow-slicetest`, 58/58 cell files). Per-cell `saRationale` names
the source fn; detailed cell IRIs live in each `structural.ttl`. Every RETAINED fn is
rowed below (no silent drops). `make validate` ✓. This batch is run_shacl-heavy
(organization 11, evidence 9, deception 12) — all run_shacl fns RETAINED.

| Slice (stereotype) | Converted fns → cells | Retained fns (reason) |
|---|---|---|
| `core/coreference` (gufo:) | 3 → 16 cells | `test_no_preferred_or_primary_coreference_terms` (whole-graph sweep); 1 run_shacl; 1 projection |
| `core/organization` (gufo:) | 5 → 19 cells | 10 run_shacl ExampleConformance; `test_no_preferred_or_primary_org_term` (whole-graph sweep); `test_change_event_type_values_exist` (cross-slice: eventType* in core/events) |
| `extensions/agentic` (gufo:) | 3 → 4 cells | 5 retained: 1 run_shacl, `test_example_answers_*` (example file + SELECT), 3 Memory/MCP runtime-integration tests |
| `core/evidence` (gufo:) | 8 → 22 cells | 9 run_shacl ExampleConformance (5 inline + 4 fixture-based) |
| `extensions/narrative` (gufo:) | 9 → 16 cells | 4 cross-slice (book-release/serial in core/documents, frame-realm in core/places, reading-order in core/documents) + a transitive merged-graph walk; 3 run_shacl + 1 negative run_shacl |
| `core/deception` (gufo:) | 22 → 31 cells | 12 run_shacl ExampleConformance; `test_bullshit_modality_exists` (cross-slice: standpoint); 1 dynamic ABox file-load; 1 competency `.rq` |

**Batch-6 tally:** 50 converted fns → 108 structural cells across 6 slices; 51
retained-with-reason (run_shacl ExampleConformance dominate, plus cross-slice subjects,
whole-graph dynamic sweeps, runtime-integration + `.rq` reads). Notable: deception's
non-assertion guardrails (NO `isFalse`/`isDeceptive` property — falsehood is a refuted
StandpointClaim, not a truth-bit) migrated as mustNot cells, and licensed-falsehood ⊄
untrue as a disjointness pair.

## #867 structural batch 5 (calendar / citations / rights / norms / teleology / lifecycle)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl` (all gufo:).
75 converted fns → 134 `gmeow:StructuralAssertion` cells; all green
(`cargo nextest -p gmeow-slicetest`, 52/52 cell files). Per-cell `saRationale` names
the source fn; detailed cell IRIs live in each `structural.ttl`. Every RETAINED fn is
rowed below (no silent drops). Closed-set vocabularies (norms group-operator /
evaluation-verdict) are split into must (seeds) + mustNot (`FILTER NOT IN` extra-member)
pairs preserving the closed-world `==` semantics. `make validate` ✓.

| Slice (stereotype) | Converted fns → cells | Retained / partial fns (reason) |
|---|---|---|
| `core/calendar` (gufo:) | 31 → 50 cells | `test_calendar_temporal_datatypes_are_datetime_or_duration` (blank-node union + cardinality); `test_calendar_axes_are_independent` (itertools.combinations 45-pair sweep — narrowing would weaken); `test_organizer_and_attendee_roles_exist` (cross-slice: roleOrganizer/roleAttendee in core/event) |
| `core/citations` (gufo:) | 9 → 11 cells | 3 run_shacl ExampleConformance; `test_contribution_with_degree_shacl_passes` (run_shacl + cross-slice Contribution/contributor); 2 self-description whole-ontology sweeps |
| `core/rights` (gufo:) | 10 → 17 cells | `test_expanded_action_vocabulary_is_seeded` (numeric `len>=45`); 3 run_shacl ExampleConformance; 6 projection-over-fixture tests |
| `extensions/norms` (gufo:) | 13 → 37 cells | `test_graft_axioms_live_extension_side_only` + `test_graft_preserves_core_trio_classhood` (cross-slice: Permission/Prohibition/Duty in core/rights); 2 run_shacl; 2 competency `.rq` reads |
| `core/teleology` (gufo:) | 7 → 14 cells | `test_intrinsic_modes_are_grounded` (partial: MentalMoment cross-slice in mentation; module-local arms converted); `test_no_preferred_or_primary_goal_terms` (whole-graph sweep); 2 run_shacl; 1 competency `.rq` |
| `core/lifecycle` (gufo:) | 5 → 5 cells | `test_supersession_properties_are_object_properties` (partial: supersedes cross-slice in coreference; supersededBy converted); `test_lifecycle_event_types_are_individuals_not_classes` (cross-slice: eventType* in core/events); 2 whole-graph sweeps; 4 run_shacl/ABox-fixture |

**Batch-5 tally:** 75 converted fns → 134 structural cells across 6 slices; 36
retained-with-reason (cross-slice subjects, run_shacl ExampleConformance, whole-graph
dynamic sweeps, numeric/cardinality + `.rq` reads). Two partials (teleology
intrinsic-modes, lifecycle supersession) split module-local arms to cells + retained
the cross-slice arm.

## #867 structural batch 4 (procedures / languages / trust / profiles / employment / risk)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl`. 37 converted
fns → 97 `gmeow:StructuralAssertion` cells; all green (`cargo nextest -p gmeow-slicetest`,
46/46 cell files). Per-cell `saRationale` names the source fn; detailed cell IRIs live
in each `structural.ttl`. Every RETAINED / PARTIAL fn is rowed (no silent drops). This
batch includes the first slices with `run_shacl` ExampleConformance tests (trust,
profiles, employment, risk) — those `run_shacl` fns are RETAINED in pytest (a different
cell type, deferred to a future example-conformance parcel). `risk` is #694-migrated
(`logic:`); the rest are `gufo:`.

| Slice (stereotype) | Converted fns → cells | Retained / partial fns (reason) |
|---|---|---|
| `extensions/procedures` (gufo:) | 14 → 18 cells | `test_ingestion_procedure_has_six_steps` (numeric cardinality count — not a scopeModule ASK) |
| `extensions/languages` (gufo:/logic:) | 5 → 32 cells | 14 retained: cross-slice (Language/FormalLanguage/WritingSystem in `core/language`; ProficiencyScale/Level in `core/kernel`; languageCode/Tag, transliterationScheme, writtenInLanguage, versionOf, bcp47Tag, langEnglish/French/Mandarin in other slices) + 7 tool-function/dynamic sweeps |
| `core/trust` (gufo:) | 6 → 13 cells | `test_contested_certification_coexists` (run_shacl ExampleConformance); `test_three_axes_are_orthogonal_in_trust` (cross-slice: accordingTo/wasAttributedTo/confidence in standpoint); `test_no_preferred_or_primary_trust_term` (whole-graph dynamic sweep) |
| `core/profiles` (gufo:) | 5 → 11 cells | 3 run_shacl ExampleConformance fns (`test_profile_shape_passes_for_wellformed_profile`, `..._fails_for_invalid_profile_applies_to`, `..._open_value_guard_warns_on_orphan`) |
| `extensions/employment` (gufo:) | 2 → 5 cells | 6 retained: cross-slice (Membership in relator slice, eventTypeHiring in `core/events`, foundedOn in `core/agreements`); 2 run_shacl ExampleConformance; 1 whole-graph dynamic sweep |
| `extensions/risk` (logic:) | 5 → 18 cells | `test_no_occurrence_gate` (multi-file ABox dynamic check); 2 run_shacl ExampleConformance; `test_competency_severity_order_query` (generated `.rq` read) |

**Batch-4 tally:** 37 converted fns → 97 structural cells across 6 slices; 31
retained-with-reason (cross-slice subjects, 8 run_shacl ExampleConformance fns deferred,
whole-graph dynamic sweeps, numeric/cardinality + `.rq` reads). First batch carrying
`run_shacl` retentions (the example-conformance cell type is a separate future parcel).

## #867 structural batch 3 (accessibility / expertise / versions / lexicon / tags / notation)

Six more home slices migrated to `slices/<g>/<n>/tests/structural.ttl` (all gufo:,
none #694-migrated). 66 converted fns → 109 `gmeow:StructuralAssertion` cells; all
green (`cargo nextest -p gmeow-slicetest`). Per-cell `saRationale` names the source
fn (`Mirrors test_*`); the detailed cell IRIs live in each `structural.ttl`. Every
RETAINED / PARTIAL / covered-elsewhere fn is rowed below (no silent drops). The
negative "ban" cells (per-value-subclass / flat-shortcut / no-bridge) are faithful
absence assertions; every `*NotFunctional` mustNot references a real in-module
`owl:ObjectProperty` (non-vacuous, verified).

| Slice (stereotype) | Converted fns → cells | Retained / partial fns (reason) |
|---|---|---|
| `extensions/accessibility` (gufo:) | 11 → 21 cells | none |
| `core/expertise` (gufo:) | 7 → 11 cells | `test_proficiency_scale_is_generalised`, `test_proficiency_levels_carry_scale` (cross-slice: ProficiencyScale in `core/kernel`; scale*/cefr*/nih* in `extensions/languages`); `test_endorsement_uses_attestation` (cross-slice: Attestation in attestation, endorses in trust); `test_no_primary_or_preferred_skill_term` (whole-graph dynamic sweep) |
| `core/versions` (gufo:) | 11 → 18 cells | `test_version_label_domain_is_entity` (cross-slice: versionLabel in `extensions/languages`); `test_membership_authority_bridges_to_vantage` (covered by observations `ex:saPropertyBridges` — the `membershipAuthority subPropertyOf vantage` triple is asserted in the observations module, not versions) |
| `extensions/lexicon` (gufo:) | 13 → 18 cells | none |
| `core/tags` (gufo:) | 13 → 22 cells | `test_no_bridge_among_has_tag_is_about_and_rdf_type` (the rdf:type-involving pairs are a dynamic/whole-graph guard; the hasTag/isAbout pair converted) |
| `core/notation` (gufo:) | 11 → 19 cells | `test_writing_system_is_sibling_not_subclass`, `test_language_is_sibling_not_subclass_of_symbolic`, `test_formal_language_not_subclass_of_notation` (cross-slice: WritingSystem/Language/FormalLanguage subjects in other slices); `test_value_vocabularies_not_subclasses` (whole-graph `subjects(subClassOf)` sweep; positive arms converted); `test_ambiguous_cases_co_modelable` (cross-slice: originFormal/LanguageOrigin in `extensions/languages`) |

**Batch-3 tally:** 66 converted fns → 109 structural cells across 6 slices; 11
retained-with-reason (7 cross-slice, 3 whole-graph dynamic sweeps, 1 covered by an
observations cell). Pytest fn deltas: accessibility 11→0, expertise 11→4, versions
12→1, lexicon 13→0, tags 14→1, notation 16→5.

## #867 structural batch 2 (provenance / sexuality / connectivity / gender / aggregation)

Five more home slices migrated to `slices/<g>/<n>/tests/structural.ttl` declarative
cells. `music` was assessed and produced **no** cell file — all 4 of its tests are
cross-slice (Genre/roles defined in creative-works/events), a generated-shapes read,
or a whole-graph `transitive_subjects` sweep — all retained in pytest; the later
tail for #694 below adds a music `structural.ttl` file for different stale
stereotype assertions, while these four batch-2 functions remain retained. RETAIN
categories applied uniformly: cross-slice subjects (module-scoped cell can't see
them), generated-artifact reads (`load_mappings`, `.rq` competency files, shapes),
whole-graph dynamic sweeps, and numeric count guards. The negative "ban" cells
(`gmeow:Woman a owl:Class` MUST be false; per-value-subclass / flat-shortcut bans)
are faithful absence assertions, not vacuous — verified each `*NotFunctional`
mustNot references a real `owl:ObjectProperty` in its module.

### `slices/core/provenance` (logic:) — 2 converted, 2 retained

| Pytest fn | DSL cell IRI | Status | Reason if retained |
|---|---|---|---|
| `test_import_activity_is_an_activity` | `ex:saImportActivityIsSubclassOfActivity` | converted | — |
| `test_activity_agent_link_is_event_safe` | `ex:saWasAssociatedWithShape` | converted | — |
| `test_carrier_and_ingestion_props` | — | **retained** | cross-slice: `sourceModifiedAt`/`contentDigest` defined in `core/sources` |
| `test_four_clocks_are_distinct_dated_annotations` | — | **retained** | cross-slice (`validFrom`/`validUntil`/`assertedAt`/`recordedNoLaterThan` in temporal/sources) + dynamic set-distinctness loop |

### `slices/core/sexuality` (gufo:) — 5 converted (14 cells), 1 retained

| Pytest fn | DSL cell IRI(s) | Status | Reason if retained |
|---|---|---|---|
| `test_orientation_facets_subclass_identity_facet` | `ex:saSexualOrientationSubclassIdentityFacet` + `ex:saRomanticOrientationSubclassIdentityFacet` | converted | — |
| `test_split_attraction_axes_are_independent` | `ex:saSplitAttractionRanges` + `ex:saSexualNotSubPropRomantic` + `ex:saRomanticNotSubPropSexual` + `ex:saOrientationPropertiesNotEquivalent` | converted | — |
| `test_orientation_values_are_individuals_not_subclasses` | `ex:saOrientationValueSubclassQualityValue` + `ex:saOrientAsexualTyped` + `ex:saRomanticAromanticTyped` + `ex:saNoPerOrientationSubclasses` | converted | — |
| `test_orientation_value_properties_functional_facets_nonfunctional` | `ex:saValuePropertiesFunctional` + `ex:saHasSexualOrientationNotFunctional` + `ex:saHasRomanticOrientationNotFunctional` | converted | — |
| `test_no_flat_orientation_shortcut` | `ex:saNoFlatOrientationShortcut` | converted | — |
| `test_competency_orientation_values_query` | — | **retained** | reads a generated `.rq` competency file + numeric `>= 16` count guard |

### `slices/extensions/connectivity` (gufo:) — 4 converted (12 cells), 3 retained

| Pytest fn | DSL cell IRI(s) | Status | Reason if retained |
|---|---|---|---|
| `test_route_class_and_kinds` | `ex:saRouteClass` + `ex:saRouteKindClass` + `ex:saRouteKindSeeds` + `ex:saRouteKindProperty` | converted | — |
| `test_connection_relator` | `ex:saConnectionClass` + `ex:saConnectionEndpoints` | converted | — |
| `test_route_properties` | `ex:saRouteEndpoints` + `ex:saRouteViaShape` + `ex:saRouteViaNotFunctional` + `ex:saHasRouteSegment` + `ex:saHasRoute` | converted | — |
| `test_reference_frame_network_graph` | `ex:saReferenceFrameNetworkGraph` | converted | — |
| `test_connects_to_universal_spine` | — | **retained** | `gmeow:connectsTo` defined in a core slice, not connectivity |
| `test_spatially_connects_to_is_symmetric_subproperty` | — | **retained** | `gmeow:spatiallyConnectsTo` cross-slice |
| `test_genealogy_subproperties_of_connects_to` | — | **retained** | `hasSpouse`/`hasSibling`/`hasParent`/`hasChild` in the genealogy slice |

### `slices/core/gender` (gufo:) — 5 converted (24 cells), 2 retained

| Pytest fn | DSL cell IRI(s) | Status | Reason if retained |
|---|---|---|---|
| `test_identity_facet_is_a_relator` | `ex:saIdentityFacetIsRelator` + `ex:saIdentityFacetNotSituation` + `ex:saGenderIdentitySubclassIdentityFacet` + `ex:saGenderExpressionSubclassIdentityFacet` | converted | — |
| `test_gender_values_are_individuals_not_subclasses` | `ex:saValueVocabsSubclassQualityValue` + `ex:saGenderSeedIndividuals` + 5×`ex:saNo*Class` | converted | — |
| `test_value_properties_are_functional_facets_nonfunctional` | `ex:saValuePropertiesFunctional` + `ex:saHasGenderIdentityShape` + `ex:saHasGenderExpressionShape` + `ex:saHasGenderIdentityNotFunctional` + `ex:saHasGenderExpressionNotFunctional` | converted | — |
| `test_no_flat_gender_shortcut` | 4×`ex:saNo*Datatype` | converted | — |
| `test_sex_assigned_at_birth_is_recorded_not_a_facet` | `ex:saSexAssignedAtBirthShape` + `ex:saSexAssignedAtBirthNotSubpropIdentity` + `ex:saNoFlatSexDatatypeProperty` + `ex:saNoFlatSexObjectProperty` | converted | — |
| `test_displayable_generalised_to_cover_identity` | — | **retained** | `gmeow:displayable` cross-slice (names slice) |
| `test_competency_gender_values_query` | — | **retained** | generated `.rq` read + numeric `>= 11` count |

### `slices/extensions/aggregation` (gufo:) — 7 converted (11 cells), 1 retained

| Pytest fn | DSL cell IRI(s) | Status | Reason if retained |
|---|---|---|---|
| `test_spatial_aggregation_is_measurement` | `ex:saSpatialAggregationIsMeasurement` | converted | — |
| `test_spatial_bin_is_place` | `ex:saSpatialBinIsPlace` | converted | — |
| `test_aggregation_function_is_value_not_subclass` | `ex:saAggregationFunctionIsValueVocab` + `ex:saAggregationFunctionSeeds` + `ex:saNoPerFunctionSubclasses` | converted | — |
| `test_aggregation_function_property_is_functional` | `ex:saAggregationFunctionPropertyFunctional` | converted | — |
| `test_has_bin_is_non_functional` | `ex:saHasBinIsObjectProperty` + `ex:saHasBinNotFunctional` | converted | — |
| `test_minimum_population_is_datatype` | `ex:saMinimumPopulationProperty` | converted | — |
| `test_no_unsafe_complex_property_chains` | `ex:saAggregationFunctionNoChain` + `ex:saHasBinNoChain` | converted | — |
| `test_contains_place_exists_and_is_inverse` | — | **retained** | `containsPlace`/`containedInPlace` defined in `core/places` |

### `slices/extensions/music` — 0 converted, 4 retained (no cell file)

| Pytest fn | Status | Reason |
|---|---|---|
| `test_genre_is_never_subclassed` | **retained** | whole-graph `transitive_subjects` dynamic sweep |
| `test_oral_tradition_guarantee` | **retained** | generated-shapes read (`gmeow-shapes.ttl`) |
| `test_dual_typed_music_roles` | **retained** | all subjects cross-slice (roles in creative-works/events) |
| `test_music_properties_functionality` | **retained** | all subjects cross-slice (`derivation*`/`hasGenre` in creative-works) |

**Batch-2 tally:** 23 converted fns → 63 structural cells across 5 slices; 11
retained-with-reason (7 cross-slice, 3 generated-artifact/competency, 1 dynamic-set) plus music's 4 retained. Pytest fn counts: provenance 4→2, sexuality 6→1, connectivity
7→3, gender 7→2, aggregation 8→1, music 4→4 (untouched).

## #694 logic stereotype retirement tail

This tail removes the remaining Python structural assertions that still expected
`gufo:` stereotypes after the canonical slice sources moved to the local
`logic:` spine. Replacement coverage is module-scoped `gmeow:StructuralAssertion`
data under `slices/**/tests/structural.ttl`, executed by the native Rust
`make slicetest` lane. No Python tests were added; migrated pytest functions were
deleted or trimmed.

| Python source | Rust slice-test replacement | Status |
|---|---|---|
| `tests/test_attestation.py` | `slices/core/trust/tests/structural.ttl` `ex:saCertificationIsRelator` | deleted entire stale Python file |
| `tests/test_employment.py` | `slices/extensions/employment/tests/structural.ttl`; `slices/core/agreements/tests/structural.ttl` | stale `Employment`/`foundedOn` gUFO assertions deleted |
| `slices/core/temporal/tests/test_temporal.py` | `slices/core/contacts/tests/structural.ttl` `ex:saInterpersonalRelationshipIsRelatorKind` | stale cross-slice gUFO assertion deleted |
| `tests/test_creative_works.py` | `slices/core/documents/tests/structural.ttl` | stale `CreativeWork`/`ContentSegmentType` gUFO assertions deleted |
| `tests/test_ai_claims.py` | `slices/core/ai/tests/structural.ttl` | stale `MemoryItem`/`Contradiction` gUFO assertions deleted |
| `tests/test_interior.py` | `slices/extensions/affect/tests/structural.ttl`; `slices/extensions/narrative/tests/structural.ttl` | stale `Emotion`/`RoleInNarrative` gUFO assertions deleted |
| `tests/test_email_*.py` | `slices/extensions/email/tests/structural.ttl` | stale `MailingList`, `EmailPatchDiff`, `CalendarMethod`, and `MessageKind` gUFO assertions deleted |
| `tests/test_language_varieties.py` | `slices/extensions/languages/tests/structural.ttl` | stale value-vocabulary and `LanguageState` gUFO assertions deleted |
| `tests/test_narrative_time.py`, `tests/test_narration.py`, `tests/test_myth.py` | `slices/extensions/narrative/tests/structural.ttl` | stale narrative-time, narration, and myth gUFO assertions deleted |
| `tests/test_rubrics.py`, `tests/test_registers.py` | `slices/extensions/norms/tests/structural.ttl` | stale `Rubric`, `Exemplar`, and `Persona` gUFO assertions deleted |
| `tests/test_verifiable_release_chain.py` | `slices/extensions/software/tests/structural.ttl` | stale `SLSALevel` gUFO assertion deleted |
| `tests/test_music_pitch.py`, `tests/test_music_collections.py`, `slices/extensions/music/tests/test_music_*.py` | `slices/extensions/music/tests/structural.ttl` | stale music stereotype assertions deleted; non-structural fixture, query, mapping, SHACL, and dynamic sweep tests retained |

**#694 tail tally:** 34 stale Python structural functions removed, 7 new
slice-local structural spec files added, 5 existing structural spec files
extended, and the native `make slicetest` lane now carries the advanced
stereotype coverage. Historical retained rows above are unchanged when they
cover generated artifacts, cross-slice subjects, numeric set/cardinality checks,
fixture SHACL, queries, or whole-graph dynamic sweeps.

## Reasoning cluster → native Rust OWL 2 RL harness (#896)

The OWL/EL/DL **reasoning + entailment** tests are a distinct lane from the
declarative slice-test DSL above: each rebuilt a reasoned rdflib graph via the
OWL-2-RL chase (`gmeow_tools.oracles.native_rl_rdflib.native_rl_closure`) and asserted a
derived triple. Because the chase is superlinear in fact count, the per-test cost
dominated the ~45-min `python` CI lane. These migrate to
`crates/logic/tests/ontology_entailments.rs` — `scoped_closure(slices, abox)`,
the native twin of `_materialize(module, *abox)`: parse the *same* authored
`module.ttl`, inject the *same* A-Box, run the native RL chase
(`gmeow_logic::reason::rl_closure`) over that small scoped input (seconds,
Docker-free). RL lane (not EL/DL) to preserve exact entailments. The structural
(asserted-graph, no-closure) tests that lived alongside them stay in pytest
pending the #867 slicetest structural migration — they are not reasoning tests.

| Pytest fn | Pytest file | Rust twin | Cell type | Status | Reason if retained | Run by |
|---|---|---|---|---|---|---|
| `test_ancestry_is_derived_not_asserted` | `tests/test_reasoning_entailments.py` | `ancestry_is_derived_not_asserted` | RL-entailment | converted | — | `make logic-test` |
| `test_location_propagates_through_containment` | `tests/test_reasoning_entailments.py` | `location_propagates_through_containment` | RL-entailment | converted | — | `make logic-test` |
| `test_suborganization_is_transitive` | `tests/test_reasoning_entailments.py` | `suborganization_is_transitive` | RL-entailment | converted | — | `make logic-test` |
| `test_proximity_measurement_is_a_measurement` | `tests/test_reasoning_entailments.py` | `proximity_measurement_is_a_measurement` | RL-entailment | converted | — | `make logic-test` |
| `test_two_axis_case_expects_inconsistency` | `tests/test_reasoning_entailments.py` | — | — | **retained** | tests the Python Docker-orchestration layer (`gmeow_tools.oracles.reasoning_cases`, monkeypatched reasoner call-order) — an independent live Python impl with no Rust twin | pytest |
| `test_two_kind_case_expects_inconsistency` | `tests/test_reasoning_entailments.py` | — | — | **retained** | same — Python orchestration of the Docker inconsistency lane | pytest |
| `test_reasoning_cases_run_all_order` | `tests/test_reasoning_entailments.py` | — | — | **retained** | same — pins the Docker reasoning-case run order | pytest |
| `test_specialized_part_relations_entail_generic_parthood` | `tests/test_mereology.py` | `specialized_part_relations_entail_generic_parthood` | RL-entailment | converted | — | `make logic-test` |
| `test_member_of_propagates_through_suborganization` | `tests/test_mereology.py` | `member_of_propagates_through_suborganization` | RL-entailment | converted | — | `make logic-test` |
| `test_event_location_propagates_through_spatial_containment_only` | `tests/test_mereology.py` | `event_location_propagates_through_spatial_containment` | RL-entailment | converted | — | `make logic-test` |
| `test_universal_part_properties_are_broad_transitive_inverses` | `tests/test_mereology.py` | — | — | **retained** | structural TBox well-formedness over the ASSERTED graph (no closure) — #867 slicetest territory, not a reasoning test | pytest |
| `test_existing_part_like_relations_specialize_the_spine` | `tests/test_mereology.py` | — | — | **retained** | structural sub-property assertions over the asserted graph — #867 | pytest |
| `test_no_winner_or_cardinality_terms_for_parts` | `tests/test_mereology.py` | — | — | **retained** | structural absence check over the asserted graph — #867 | pytest |
| `test_competency_ancestry_is_answered_only_by_reasoning` | `tests/test_competency.py` | `ancestry_is_derived_not_asserted` | RL-entailment | converted | — (same genealogy chain as the `_materialize` ancestry twin) | `make logic-test` |
| `test_place_naming_is_entailed_not_asserted` | `tests/test_competency.py` | `place_naming_is_entailed_not_asserted` | RL-entailment | converted | — (the first `owl:equivalentClass` defined-class classification + its negative) | `make logic-test` |
| the 47 `_query_terms` / `_query_terms_on_graph` competency QUERY tests | `tests/test_competency.py` | — | — | **retained (de-reasoned)** | answer on the ASSERTED merged graph via SPARQL property paths — they never needed the OWL-RL closure; `_query_terms` was repointed from `_reasoned_graph()` to the asserted `_merged_graph()`, killing the ~4-min materialization. Stay in pytest pending the #867 slicetest competency migration | pytest (now ~1.7s) |
| `test_sensory_observation_specialises_observation` | `tests/test_sensory.py` | `sensory_observation_specialises_observation` | RL-entailment | converted | — (SensoryObservation ⊑ Observation) | `make logic-test` |
| `test_sensor_specialises_agent` | `tests/test_sensory.py` | `sensor_specialises_agent` | RL-entailment | converted | — (Sensor ⊑ Agent) | `make logic-test` |
| `test_sensory_quantity_inherits_scalar_quantity` | `tests/test_sensory.py` | `sensory_quantity_inherits_scalar_quantity` | RL-entailment | converted | — (SensoryQuantity ≡ ScalarQuantity) | `make logic-test` |
| `test_sensory_observation_el_axioms` | `tests/test_sensory.py` | `sensory_observation_el_axioms_stay_consistent` | RL-entailment | converted | — (full SensoryObservation survives materialization) | `make logic-test` |
| `test_sensory_quantity_frame_inheritance` | `tests/test_sensory.py` | `sensory_quantity_frame_inheritance` | RL-entailment | converted | — (isResultOf ∘ hasReferenceFrame chain) | `make logic-test` |
| `test_has_sensory_quantity_property_chain` | `tests/test_sensory.py` | `has_sensory_quantity_property_chain` | RL-entailment | converted | — (hasSensoryObservation ∘ sensoryResult shortcut) | `make logic-test` |
| `test_contested_sensory_readings_coexist` | `tests/test_sensory.py` | `contested_sensory_readings_coexist` | RL-entailment | converted | — (Principle 9 coexistence; decimal literals omitted, not assertion-relevant) | `make logic-test` |
| the 14 structural `test_sensory.py` tests (TBox existence, equivalentClass/subProperty/inverseOf assertions, SOSA/AFO mappings) | `tests/test_sensory.py` | — | — | **retained** | structural checks over the ASSERTED graph / generated mapping artifacts (no closure) — #867 slicetest territory | pytest (now ~4.8s) |
| `test_coordinate_observation_chain_fires` | `tests/test_places.py` | `coordinate_observation_chain_fires` | RL-entailment | converted | — (hasCoordinateObservation ∘ coordinateResult ⊑ hasCoordinates) | `make logic-test` |
| `test_geometry_observation_chain_fires` | `tests/test_places.py` | `geometry_observation_chain_fires` | RL-entailment | converted | — (hasCoordinateObservation ∘ geometryResult ⊑ hasGeometry) | `make logic-test` |
| `test_sensory_environment_el_axioms_fire` | `tests/test_sensory_environment.py` | `sensory_environment_el_axioms_fire` | RL-entailment | converted | — (environmentAtLocation domain ⇒ SensoryEnvironment) | `make logic-test` |
| `test_sensory_perception_specialises_standpoint_claim` | `tests/test_sensory_environment.py` | `sensory_perception_specialises_standpoint_claim` | RL-entailment | converted | — (SensoryPerception ⊑ StandpointClaim, ⊑ Observation) | `make logic-test` |
| `test_mental_reference_frame_specialises_reference_frame` | `tests/test_sensory_environment.py` | `mental_reference_frame_specialises_reference_frame` | RL-entailment | converted | — (MentalReferenceFrame ⊑ ReferenceFrame) | `make logic-test` |
| `test_frame_inheritance_via_coordinate_matrix` | `tests/test_sensory_environment.py` | `frame_inheritance_via_coordinate_matrix` | RL-entailment | converted | — (CoordinateMatrix inherits the observation frame) | `make logic-test` |
| `test_mental_reference_frame_requires_host` | `tests/test_sensory_environment.py` | — | — | **retained** | MIXED: a structural blank-node `someValuesFrom` restriction-axiom check (asserted graph) + a small scoped consistency check; the inference half duplicates the migrated ReferenceFrame twin | pytest |
| `test_observation_el_axioms_fire` | `tests/test_observations.py` | `observation_el_axioms_fire` | RL-entailment | converted | — (Observation EL consistency) | `make logic-test` |
| `test_frame_inheritance_property_chain` | `tests/test_observations.py` | `observation_frame_inheritance_property_chain` | RL-entailment | converted | — (inverse(observationResult) ∘ hasReferenceFrame) | `make logic-test` |
| `test_measurement_specialises_observation` | `tests/test_observations.py` | `measurement_specialises_observation` | RL-entailment | converted | — | `make logic-test` |
| `test_sensory_observation_specialises_observation` | `tests/test_observations.py` | `sensory_observation_specialises_observation` | RL-entailment | converted | — (same subsumption as the `test_sensory.py` twin) | `make logic-test` |
| `test_standpoint_claim_specialises_observation` | `tests/test_observations.py` | `standpoint_claim_specialises_observation` | RL-entailment | converted | — | `make logic-test` |
| `test_name_usage_specialises_observation` | `tests/test_observations.py` | `name_usage_specialises_observation` | RL-entailment | converted | — (universal claim construct, #69) | `make logic-test` |
| `test_identity_facet_specialises_observation` | `tests/test_observations.py` | `identity_facet_specialises_observation` | RL-entailment | converted | — (#69) | `make logic-test` |
| `test_rights_statement_specialises_observation` | `tests/test_observations.py` | `rights_statement_specialises_observation` | RL-entailment | converted | — (#69) | `make logic-test` |
| `test_kin_relationship_specialises_observation` | `tests/test_observations.py` | `kin_relationship_specialises_observation` | RL-entailment | converted | — (#69) | `make logic-test` |
| `test_is_result_of_provenance_chain` | `tests/test_observations.py` | `is_result_of_provenance_chain` | RL-entailment | converted | — (isResultOf inverse of observationResult, #77) | `make logic-test` |
| `test_frame_inheritance_via_quantity` | `tests/test_observations.py` | `frame_inheritance_via_quantity` | RL-entailment | converted | — (#77) | `make logic-test` |
| `test_stream_el_axiom` | `tests/test_observations.py` | `stream_el_axiom_stays_consistent` | RL-entailment | converted | — (#96) | `make logic-test` |
| `test_spatial_measurement_infers_observation` | `tests/test_observations.py` | `spatial_measurement_infers_observation` | RL-entailment | converted | — (#125) | `make logic-test` |
| `test_coordinate_observation_infers_spatial_measurement` | `tests/test_observations.py` | `coordinate_observation_infers_spatial_measurement` | RL-entailment | converted | — (#125) | `make logic-test` |
| `test_coordinate_observation_frame_inheritance` | `tests/test_observations.py` | `coordinate_observation_frame_inheritance` | RL-entailment | converted | — (#125) | `make logic-test` |
| `test_coordinate_observation_el_axioms` | `tests/test_observations.py` | `coordinate_observation_el_axioms_stay_consistent` | RL-entailment | converted | — (#125) | `make logic-test` |
| `test_quality_assessment_specialises_observation` | `tests/test_quality.py` | `quality_assessment_specialises_observation` | RL-entailment | converted | — (QualityAssessment ⊑ Observation; the assessedEntity/Place A-Box is decoration) | `make logic-test` |
| `TestReasonNativeEngine::test_consistent_with_entailments_and_gaps` | `tests/test_reason_native.py` | `reason_all` (`crates/logic/src/reason/mod.rs`) + the `check-generated` byte-regen of the committed closure/ledger | engine-direct | converted (covered) | — (consistency + non-empty closure is the `reason_all` unit test; real-bundle consistency/gaps is pinned by the byte-regenerable `generated/logic/*.ttl` gate) | `make rust-test` + `gmeow-dev check-generated` |
| `TestVerifyNative::test_violating_query_reports_error` | `tests/test_reason_native.py` | `violating_query_yields_error_finding_with_detail` (`crates/logic/src/verify.rs`) | engine-direct | converted (covered) | — (the verify violation path — same `verify()` code + `!ok`/`error_count`/finding-code semantics) | `make logic-test` |
| `TestNativeReasonArtifacts::test_closure_parses_and_carries_reifier_provenance` | `tests/test_reason_native.py` | `closure_emits_triple_and_reifier_with_provenance` (`crates/logic/src/reason/artifacts.rs`) | engine-direct | converted (covered) | — (same `reifies`/`Deduction`/`viaRule` tokens on the same `build_closure_ttl` output) | `make logic-test` |
| `TestNativeReasonArtifacts::test_explanations_parse_and_carry_derivation_skeleton` | `tests/test_reason_native.py` | `explanations_emit_derivation_with_premise` (`crates/logic/src/reason/artifacts.rs`) | engine-direct | converted (covered) | — (same `Derivation`/`concludes`/`hasPremise` tokens) | `make logic-test` |
| `TestNativeReasonArtifacts::test_ledger_parses_and_carries_entries_gaps_and_counts` | `tests/test_reason_native.py` | `ledger_header_entries_gaps_and_counts` (`crates/logic/src/reason/artifacts.rs`) + the byte-regen gate | engine-direct | converted (covered) | — (`CrosscheckLedger`/`DlGap`/`entailmentCount` tokens; the `#666`/`classic-cross-check`/`consistent> true` banner tokens are byte-pinned by `check-generated`) | `make logic-test` + `gmeow-dev check-generated` |
| `TestNativeReasonArtifacts::test_artifacts_are_byte_regenerable_against_committed` | `tests/test_reason_native.py` | `crates/pipeline/tests/full_parity.rs` + the `ontology-generated` lane (`gmeow-dev check-generated`) | engine-direct | converted (covered) | — (the SAME `GtsGraphStore → reason_all → build_*_ttl` path reproduces the committed `generated/logic/*.ttl` EXACTLY; the authoritative cutover gate, fail-closed) | `gmeow-dev check-generated` |
| `TestReasonNativePipeline::test_report_ok_and_writes_closure` | `tests/test_reason_native.py` | — | — | **retained** | the Python report wrapper `gmeow_tools.reason.reason_native` — disk-writing orchestration over the Rust core; independent live Python surface, no Rust twin (doctrine-guard) | pytest |
| `TestVerifyNative::test_clean_over_bundle_and_writes_artifacts` | `tests/test_reason_native.py` | — | — | **retained** | the Python report wrapper `gmeow_tools.reason.verify_native` — slice-query glob discovery + JSON/SARIF/HTML artifact writing; Python surface, no Rust twin (doctrine-guard) | pytest |
| `test_gmeow_reason_native` | `tests/test_mcp_server.py` | — | — | **retained** | the thin Python MCP wrapper `gmeow_tools.mcp_server.gmeow_reason` — independent live Python surface, no Rust twin (doctrine-guard) | pytest |

**#896 reasoning tally:** 45 converted (39 RL-entailment twins + 6 engine-direct
covered by existing Rust unit tests / the byte-regen gate), 11 retained-with-reason
(3 Python Docker-orchestration, 4 structural-not-reasoning, 1 mixed, 2 native-report
Python wrappers, 1 MCP wrapper), plus the 47 competency QUERY tests de-reasoned in
place. **Files fully reasoning-free now:** `test_reasoning_entailments.py`,
`test_mereology.py`, `test_competency.py`, `test_sensory.py`, `test_places.py`,
`test_observations.py`, `test_quality.py`. `test_reason_native.py` carries only the
two Python report-wrapper tests (8 → 2; ~330 s → ~105 s).
The migrated `_materialize` helpers and A-Box-injection imports were removed from
the touched pytest files; `tests/test_competency.py` dropped from ~14 min to
**~1.7 s** (the two ~5–9 min reasoning tests gone, the closure cost removed).

## Other slices

No other slice carries declarative test-DSL specs yet (T2 authored only the
epistemics exemplars). Their pytest tests are therefore **retained, pending
future slice migration** — removing them now would drop coverage with no
declarative twin to replace it. The ~1,950 ontology-data rdflib tests across the
remaining slices are migrated in follow-on parcels under epic #781; the harness
already discovers and runs the three fixed spec filenames (`competency.ttl`,
`structural.ttl`, `example-conformance.ttl`) under any slice, so each new slice
spec lights up automatically.

## #867 — `tests/test_competency.py` → native competency cells (2026-06-24)

Supersedes the "Other slices" note above for the **competency** cell type: the
whole of `tests/test_competency.py` (~46 functions + the QC `missing-definitions`
check) is migrated to declarative `competency.ttl` cells across the owning slices.
A new harness capability — `gmeow:cqDataFile` (a slice-relative ABox overlay,
symmetric with `gmeow:exampleFile`; `crates/slicetest/src/{dsl,exec}.rs` +
`dsl/tests/vocabulary.ttl`) — lets the instance-classifier questions migrate as
real native cells instead of pytest retains.

**Faithfulness encoding.** The TBox `_query_terms` tests asserted a *subset* of
terms present (robust to additions), so each migrated cell enumerates exactly the
pytest-asserted rows with `cqExactRows` **omitted** (contains-check) — never
strengthened to an exact closed set. The five `len() >= N` floors (`life-events`
>=25, `citation-intents` >=10, `deception-types` >=10, `deception-roles` >=5,
`myths` >=5) are coarse bounds the DSL has no operator for; the named rows are
pinned and the loose floor is dropped (a faithful relaxation). The
deliberately-empty queries pin `cqExpectRowCount 0`.

| Owning slice | cell(s) | source `_query_terms*` test(s) | encoding |
|---|---|---|---|
| `core/deception` | `cqDeceptionTypes`, `cqDeceptionRoles` + 8 single-event classifiers + `cqDeceptionSelfDeception` (2-var) + `cqDeceptionLicensedFalsehood` (count 0) | deception-types (was `>=10`) / deception-roles (was `>=5`) + lie/omission/paltering/bullshit/distortion/self-deception/fabrication/forgery/impersonation/licensed-falsehood | subset + 9 `cqDataFile` overlays + 1 empty |
| `extensions/narrative` | `cqMyths` | myths (was `>=5`) | subset |
| `core/expertise` | `cqExpertProficiency`, `cqEndorsedVsSelfAsserted` (3 rows), `cqEmploymentCredentials` | expertise expert-python / endorsed-vs-self / employment-credentials | 3 `cqDataFile` overlays (full multi-var rows) |
| `core/names` | appellation-kinds, name-part-types, pronoun-sets, place-namings | same | subset |
| `core/places` | location-kinds, place-types, storage-media, place-properties | same | subset |
| `extensions/languages` | language-origins, writing-systems, proficiency-levels | same | subset |
| `core/creative-works` | works | works | subset |
| `core/rights` | rights | rights | subset |
| `extensions/genealogy` | kinship | kinship | subset |
| `core/events` | life-events | life-events (was `>=25`) | subset |
| `extensions/email` | email-participants | email-participants | subset |
| `core/contacts` | interpersonal-relationships | interpersonal-relationships | subset |
| `core/trust` | message-trust, key-schemes, key-certifications, trust-assertions | same | subset |
| `core/sources` | import-provenance | import-provenance | subset |
| `core/temporal` | temporal-provenance-clocks | temporal-provenance-clocks | subset |
| `core/citations` | citation-intents | citation-intents (was `>=10`) | subset |
| `core/evidence` | evidence, notability-eligible | same | subset |
| `extensions/procedures` | procedures, ingestion-executions (count 0) | same | subset + 1 empty |
| `core/inquiry` | research-inquiries (count 0) | research-inquiries | empty |
| `core/quality` | `cqMissingDefinitions` (count 0) | `test_qc_missing_definitions_is_empty` | empty; preserves the OPEN-universal dynamic `FILTER(STRSTARTS(...))` (the #869 Gap-1 GOOD pattern, NOT a blacklist) |

**Instance-overlay reconciliation:** the source had 14 `_query_terms_on_graph`
classifiers = 10 deception + 4 expertise. **13 are migrated** as `cqDataFile`
overlay cells; **1 is a documented retain** (below).

**Retained (1):** `test_competency_expertise_expiring_credentials_query` stays in
`tests/test_competency.py`. Its query selects credentials whose `gmeow:validUntil`
is within one year of `NOW()` — a clock-RELATIVE window. No static fixture date
satisfies "within a year of now" perpetually (a far-future literal falls outside
the window; a fixed near date becomes a time-bomb that silently reds as wall-clock
time passes it), so a faithful native cell would need clock-relative date
templating the DSL deliberately lacks. Per the verification-honesty doctrine
(never author a test that silently breaks later) it remains a pytest retain that
builds its data relative to the run-time clock. `tests/test_competency.py` is
trimmed to this one function + the `_query_terms_on_graph` helper (not deleted).

**#869 Gap-1:** none of the migrated cells is a finite-VALUES blacklist standing
in for an open universal. The TBox cells enumerate fixed term sets the source
already pinned; the overlay cells pin fixed instance classifications; the QC cell
references the `.rq` verbatim, keeping its dynamic `FILTER(STRSTARTS(...))` open
universal over every present-or-future `gmeow:` class.

**Red-proofs (2026-06-24).** Each new `competency.ttl` was proven to red under a
transient break, then reverted: deception `cqDeceptionLie` → `expected row(s)
absent {?event=…event1}` (broken fixture); narrative `cqMyths`, names
`cqAppellationKinds`, places `cqLocationKinds`, languages `cqLanguageOrigins`, and
the 13 Task-5 cells → row absent (corrupted term); expertise `cqExpertProficiency`
→ full multi-var row absent (non-expert level); the count-0 cells (procedures
ingestion, inquiry, quality QC) → `expected 1 rows, got 0` (count flipped to 1).
All authored by MEASURING the live merged store (and merged+fixture for overlays),
never by guessing.

**Verification:** `cargo nextest run -p gmeow-slicetest` runs 25 competency files
green; the harness auto-discovers each new `competency.ttl` (no harness wiring
change beyond the Task-1 `cqDataFile` addition).

## Notes / known limitations

- **Competency reasoning cost.** The competency lane defaults to the *asserted*
  merged graph (`gmeow:reasoningNone`): no materialization, with SPARQL property
  paths supplying transitive closure at query time — a sub-second graph build
  (the epistemics competency case runs in ~0.3s). A question that needs
  type/subsumption entailment opts into `gmeow:reasoningRdfs`, which closes the
  merged graph under RDFS natively in oxigraph (iterated `CONSTRUCT` to a
  fixpoint, `crates/slicetest/src/stores.rs`) — seconds, built once per spec file
  and only when a question in that file requests it. The harness deliberately
  does **not** run the ~4-minute OWL 2 RL chase `tests/test_competency.py` pays,
  and carries no `gmeow-logic`/Nemo dependency. Reasoning is monotonic, so the
  asserted default can only ever *under*-answer — a loud set/count mismatch,
  never a silent wrong-green. See `docs/TESTING.md`.
- **`gmeow:saShape` is not yet executed.** No T2 exemplar exercises shape-based
  structural assertions, so `run_structural_cell` hard-fails on `saShape` rather
  than silently passing (no-optionality doctrine). Likewise inline `gmeow:cqQuery`
  and `gmeow:cqExpectAsk` have no exemplar; they are implemented and unit-covered
  but unexercised by a slice spec.
