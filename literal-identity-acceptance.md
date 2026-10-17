# Literal identity acceptance assessment

Issue 1755 explicitly permits independent adoption of this representation repair;
general source coverage, native reasoning and FOF transfer remain separate.
Principles 2, 3, 4 and 7 govern the complete native literal representation.

The implementation and cross-path controls are already merged in immutable main
ancestor eca80ae008d24dccf5b4cd19011e0c7acf28d3b8. This review does not add another
copy or PR. The current branch changes graph-selection consumers, not the native
literal representation. The retired obj String / obj_is_literal pair is absent
from the compiler; AtomicTerm::Literal carries the complete native RdfLiteral.
Formula::as_horn_axiom clones that native literal and the direct frontend routes
native terms through the same checked compact carrier. Cache tuples retain all
four components and reject incoherent combinations. Term keys frame all fields.

| Requirement | Current source/control evidence |
| --- | --- |
| Direct, standalone, nested and rule paths agree | frontend/native_literal_tests/acceptance.rs sources and authored_literal_paths_preserve_identity_through_gmeow_codecs |
| Datatype, language, direction and numeric spelling remain distinct | gmeow_identity_keys_normalize_spelling_without_numeric_value_coercion checks seven distinct identities after actual deduplication |
| Plain/explicit string and language case normalize | Same identity groups include plain versus xsd:string and en versus EN; checked_literal_constructors_share_normalized_identity_with_authored_formula_terms verifies constructors |
| Serialization and reconstruction preserve objects and enclosing identities | JSON LogicProgram/Formula restoration, canonical RDF projection/frontend inverse, relational-core projection/parser with exact keys and atoms |
| Signed and quoted facts retain assertion status | signed_literals_keep_the_same_complete_object_without_cross_identity_collapse; quoted_literal_propositions_do_not_assert_their_payload |
| Rule readers preserve variable versus literal kind | compact_term_tests::rule_variables_and_question_mark_literals_remain_distinct_through_every_codec covers native RDF, CLIF, CGIF and XCL |
| A weaker target cannot silently truncate | datalog_omits_whole_rules_instead_of_equating_distinct_typed_literals verifies explicit whole-rule loss; malformed typed carriers reject the whole rule |

Fresh execution PASSED through make nextest-synthetic with only the two synthetic
frontend modules selected: 16 passed, 0 failed, 987 unselected tests skipped,
0.185s execution after the shared test-profile build (8m24s). Nextest run
708bb8e7-55e6-4d39-8c9d-b4693389b220. The source corpus-purity seal also passed.
No authenticated corpus access or production occurred. The exact result is
literal-identity-current.log, executed on 271463204 with native pin af8936e6.
Existing strict workspace Clippy passed at the same native pin; normal commit
hooks passed for the graph-only changes. This is focused representation
acceptance, not a claim that the pending full hosted workflow has passed.

Integration assessment: origin/main remains 9cb2cb1fc. Its separate declaration
fix changes probability configuration admission, source capability guards and
associated tests/prose. The literal AtomicTerm/codec and both selected acceptance
modules are unchanged. The direct frontend diff removes a probability-declaration
check in extract_contracts; it does not change literal extraction, lowering or
normalization. The complete graph-integration candidate merges cleanly as tree
5b1252380acfa3799483508ea3613150de8e3705. Hosted pull-request checkouts use the
merge candidate. No base synchronization or CI restart is needed merely for lag.
The native RdfLiteral definition and dataset parser are unchanged between main's
7734b013 pin and the tested af8936e6 pin (exact source diff checked). The immutable
main adoption revision is 9cb2cb1fc97a55605d818f10c1e5912e25cc8a75; the literal
repair originally landed through eca80ae00. No consumer-specific external prover
run is required by this issue's explicitly agreed representation-only contract.

VERDICT: the independent-adoption representation requirements are met. Close
1755 with this execution evidence; retain the full logic goal and all broader
source/engine/prover requirements as open work. Full hosted CI is still pending.

This is an owner acceptance assessment, not an independent review. Closing this
representation defect would not close the full logic goal, issue 1803, source
admission completeness, whole-canon consistency or independent prover acceptance.
