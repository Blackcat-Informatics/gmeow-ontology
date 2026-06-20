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

- `gmeow:CompetencyQuestion` — SPARQL ASK/SELECT over the OWL-2-RL-reasoned
  merged ontology (the same reasoned-graph lane `tests/test_competency.py` uses).
- `gmeow:StructuralAssertion` — SPARQL ASK over the slice module (± examples).
- `gmeow:ExampleConformance` — native SHACL over module + shapes + the example.

## `slices/core/epistemics`

| Pytest fn | Pytest file | DSL cell IRI | Cell type | Status | Reason if retained | Verified by |
|---|---|---|---|---|---|---|
| `test_knows_that_subproperty_of_believes` | `tests/test_epistemics.py` | `ex:saKnowsThatSubPropertyOfBelieves` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_spine_are_object_properties_with_agent_domain` | `tests/test_epistemics.py` | `ex:saSpineObjectPropertiesWithAgentDomain` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_spine_have_open_range` | `tests/test_epistemics.py` | `ex:saSpineOpenRange` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_proposition_is_a_social_object` | `tests/test_epistemics.py` | `ex:saPropositionIsSocialObject` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_spine_properties_are_not_functional` | `tests/test_epistemics.py` | `ex:saSpineNotFunctional` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_no_factivity_no_truth_bit` | `tests/test_epistemics.py` | `ex:saNoTruthBit` + `ex:saSpineOpenRange` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_doxastic_standpoint_claim_is_subclass_of_standpoint_claim` | `tests/test_epistemics.py` | `ex:saDoxasticStandpointClaimSubclass` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_claim_of_belief_is_functional_object_property` | `tests/test_epistemics.py` | `ex:saClaimOfBeliefFunctionalObjectProperty` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_defeated_by_has_status_range` | `tests/test_epistemics.py` | `ex:saDefeatedByStatusRange` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_justification_status_individuals_exist` | `tests/test_epistemics.py` | `ex:saJustificationStatusIndividuals` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_credence_and_confidence_are_distinct` | `tests/test_epistemics.py` | `ex:saCredenceDomainDoxasticState` + `ex:saConfidenceNotConflatedWithCredence` | StructuralAssertion | converted | — | `run_structural_file` |
| `test_flagship_example_parses` | `tests/test_epistemics.py` | — | — | **retained** | the flagship references cross-slice classes; the harness's slice-scoped ExampleConformance emits `shacl.ClassConstraintComponent` for the unresolved cross-slice `sh:class` targets. Validated in full by `make validate`. | pytest |
| `test_justified_by_has_named_domain_and_range` | `tests/test_epistemics.py` | `ex:saJustifiedByDomainRange` + `ex:saJustifiedByNotFunctional` | StructuralAssertion | **partial** | the exact `owl:unionOf` set membership stays in pytest as `test_justified_by_union_membership` (an "these members and no others" check the ASK form can't express) | `run_structural_file` + pytest |
| `test_justification_terms_are_annotated` | `tests/test_epistemics.py` | — | — | **retained** | universal over *dynamically discovered* `JustificationStatus` individuals (open set); backstops the make-validate annotation contract | pytest |
| `test_every_term_is_annotated` | `tests/test_epistemics.py` | — | — | **retained** | annotation-completeness sweep; backstops the make-validate annotation contract | pytest |
| `test_suppression_round_trip` | `tests/test_epistemics.py` | — | — | **retained** | numeric credence comparison + temporal tenure navigation; not expressible as a SPARQL ASK | pytest |
| `test_epistemics_mapping_set_exists_and_has_expected_rows` | `tests/test_epistemics.py` | — | — | **retained** | reads a GENERATED artifact (SSSOM TSV) outside the ontology graph | pytest |
| `test_competency_agents_query` | `tests/test_competency.py` | `ex:cqAgentKinds` | CompetencyQuestion | converted | — | `run_competency_file` |
| `test_competency_contribution_roles_query` | `tests/test_competency.py` | `ex:cqContributionRoles` | CompetencyQuestion | converted | — | `run_competency_file` |
| (T2: the missing-agent counter-example) | `tests/test_epistemics.py` (historical) | `ex:ecDoxasticStateMissingAgent` | ExampleConformance | converted (T2) | — | `run_conformance_file` |

**Epistemics tally:** 14 converted (11 structural + 2 competency + 1 T2
counter-example), 1 partial, 5 retained-with-reason.

## Other slices

No other slice carries declarative test-DSL specs yet (T2 authored only the
epistemics exemplars). Their pytest tests are therefore **retained, pending
future slice migration** — removing them now would drop coverage with no
declarative twin to replace it. The ~1,950 ontology-data rdflib tests across the
remaining slices are migrated in follow-on parcels under epic #781; the harness
already discovers and runs any `slices/**/tests/*.ttl` that appears, so each new
slice spec lights up automatically.

## Notes / known limitations

- **Competency reasoning cost.** The competency lane closes the merged ontology
  under OWL 2 RL via the native chase — the *same* chase `tests/test_competency.py`
  already pays in the gate today (≈4 min cold). It is memoized in a
  content-addressed disk cache (`crates/slicetest/src/reasoned.rs`), so a
  `make check` runs it at most once regardless of how many slices carry
  competency specs.
- **`gmeow:saShape` is not yet executed.** No T2 exemplar exercises shape-based
  structural assertions, so `run_structural_cell` hard-fails on `saShape` rather
  than silently passing (no-optionality doctrine). Likewise inline `gmeow:cqQuery`
  and `gmeow:cqExpectAsk` have no exemplar; they are implemented and unit-covered
  but unexercised by a slice spec.
