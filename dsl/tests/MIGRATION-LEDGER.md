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
