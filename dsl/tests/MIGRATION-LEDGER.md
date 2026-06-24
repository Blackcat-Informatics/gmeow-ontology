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
| `test_metacognitive_state_is_a_mental_moment_kind` | `tests/test_metacognition.py` | `ex:saMetacognitiveStateIsMentalMomentKind` + `ex:saMetacognitiveStateNoExtraGufoMetaclass` | StructuralAssertion | converted | — | `make slicetest` |
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
| `test_interpersonal_relationship_is_a_gufo_relator` | `tests/test_temporal.py` | — | — | **retained** | CROSS-SLICE: `gmeow:InterpersonalRelationship` (core/contacts, core/names) ⊑ gufo:Relator — declared in those slices, absent from temporal/module.ttl. Merged-graph integration check. | pytest |

**Temporal tally:** 6 converted, 2 retained-with-reason (cross-slice merged-graph). Source file `tests/test_temporal.py` trimmed to the 2 retained functions (not deleted).

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

## Reasoning cluster → native Rust OWL 2 RL harness (#896)

The OWL/EL/DL **reasoning + entailment** tests are a distinct lane from the
declarative slice-test DSL above: each rebuilt a reasoned rdflib graph via the
OWL-2-RL chase (`gmeow_tools.native_rl_rdflib.native_rl_closure`) and asserted a
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
| `test_two_axis_case_expects_inconsistency` | `tests/test_reasoning_entailments.py` | — | — | **retained** | tests the Python Docker-orchestration layer (`gmeow_tools.reasoning_cases`, monkeypatched reasoner call-order) — an independent live Python impl with no Rust twin | pytest |
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

**#896 reasoning tally (running):** 39 converted, 8 retained-with-reason (3 Python
Docker-orchestration, 4 structural-not-reasoning, 1 mixed structural/consistency),
plus the 47 competency QUERY tests de-reasoned in place and the per-slice
structural tests retained (no closure; #867 will move them to slicetest cells).
**Files fully reasoning-free now:** `test_reasoning_entailments.py`,
`test_mereology.py`, `test_competency.py`, `test_sensory.py`, `test_places.py`,
`test_observations.py`. **Still pending (stage 2):** `test_reason_native.py`
(engine-direct, asserts over `reason_all` output — a different shape than the
scoped-closure entailment tests) and `test_mcp_server.py::test_gmeow_reason_native`
(thin Python MCP wrapper).
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
