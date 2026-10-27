# Issue #1999: Use native JSON for complete correspondence physical-plan producer reports

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move the complete correspondence physical-plan producer report to PurRDF native JSON under Principles 4 and 7, including its actual publisher and authenticated typed reader.

Acceptance:

- Cover all six records: CompositionPlanLimits, CompositionOptimizationScope, CompositionLogicalStage, CompositionPlanCost, CompositionPlanCertificate and CompositionPlanReport. Cover all CompositionStageCheck, OriginalPlanReason and CompositionPhysicalPlan variants. Runtime execution rows, receipts and the outer CompositionExecution operation remain separate.
- Wire PreparedCompositionProgram::report_json, the actual CompileLogicStage artifact publisher and authenticated typed report reader through native JSON. Preserve two-space pretty layout and no terminal newline. Make infallible serialization explicit and update its actual callers without retaining the obsolete serialization-error path.
- Retain all shared Serde definitions, attributes and variant order for the genuine separate public CLI inspection, certificate and execution JSON owners. Do not alter or claim qualification of that asset-bound consumer.
- Preserve every field and vector order, raw identity/path strings and strict checked usize counts, including all three cost counts and max_path_nodes. Limits fields remain required despite the Default implementation. Stage standpoint accepts missing/null and writes explicit null. Require named records and tagged objects; refuse positional forms and known duplicate or malformed fields while tolerating unknown fields under the native default depth policy.
- Preserve JSON tag names independently of structural versioned IDs. Stage-check tags omit the structural -v1 suffix; original reasons are search-limit and native-admission. CompositionPhysicalPlan uses internally tagged objects: kind=original with required reason, or kind=fused-witness-join. Dispatch on kind before reading variant fields: duplicate kind always fails, Original requires exactly one typed reason, and FusedWitnessJoin ignores unknown fields including malformed or repeated reason entries. External unit enums retain string and singleton-null-object forms.
- Preserve structural StableDigest framing, all planning/query/execution algorithms, deterministic limits and every explicit certificate/report verification call. Hydration retains raw report data without re-planning, executing, verifying, recomputing or repairing certificates.
- Extend the existing authored fusion and certificate-rebinding controls through actual native report bytes while retaining every original result, middle-witness and forgery assertion. Retain the composite-body and standpoint refusal controls, and add bounded complete-wire/pretty-no-newline and variant-admission coverage. Compile actual logic, pipeline and developer CLI callers and run normal hooks. Tests use explicit authored in-memory data only, with no canonical corpus construction or access.

Remaining contracts: #1776, #1777, #1336 and #1962. Separate public CLI and runtime serialization, complete program/report migration, workspace Serde retirement and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

