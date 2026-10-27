# Issue #1997: Use native JSON for complete plan projection producer artifacts

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move both complete plan-projection producer artifacts to PurRDF native JSON under Principles 4 and 7: `generated/logic/rchops21-plan-projection.json` and `generated/logic/rchops21-plan-recovery.json`.

Acceptance:

- Cover the full sixteen-record payload family: PlannedStep, PlannedEdge, ProjectedOutcome, ProjectedExpressionFact, ProjectedExpression, ProjectedConditionalStep, ProjectedAction, ProjectedGuard, PlanLiteral, FreshnessRequirement, ProjectedCarrierFact, PlanProjectionComplement, PlanProjectionCertificate, PlanProjection, ObservedPlanOccurrence and PlanRecovery. Cover all PlannedStepKind, PlannedEdgeKind and PlanRecoveryVerdict variants. PlanProjectionLimits remains a separate input policy.
- Wire both actual producer artifact publishers and both authenticated typed readers through the native owner. Preserve Serde-compatible two-space pretty layout, declaration field order and exactly one trailing newline. Retain all shared Serde definitions, attributes and variant order for the genuine separate public correspondence CLI JSON owner; do not modify or claim qualification of that asset-bound consumer.
- Preserve every vector's order, optional missing/null absence and explicit null output. Nonoptional fields remain required. Use checked strict usize admission for loop count, cycle and position. Tolerate unknown fields; refuse known duplicates, positional records, malformed counts and malformed variants under the native default depth policy.
- Preserve exact kebab-case tags: action/guard, serial/guard-then/guard-else/next-iteration and planned-skeleton-recovered. Preserve PlanLiteral as a named raw-string DTO, with datatype, language and direction strings verbatim. Do not apply the checked IR literal tuple or direction-enum normalization. Retain rendered RDF subject/object strings without reparsing.
- Keep structural StableDigest algorithms unchanged for projection, recovery, certificate and literal identities. Hydration must not verify, recompute or repair certificates. Preserve existing explicit verification calls and every authenticated artifact assertion.
- Extend the two existing safe authored projection/recovery controls through complete native transport while retaining their verification and forgery assertions. Retain the bounded-unrolling refusal control and add bounded full-family wire/pretty/newline and malformed-admission coverage. Compile actual logic, pipeline and developer CLI callers and run normal hooks. Use explicit authored in-memory datasets only, with no canonical corpus production or execution.

Remaining contracts: #1776, #1777, #1336 and #1962. The separate public CLI JSON operation, complete program/report migration, workspace Serde retirement and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

