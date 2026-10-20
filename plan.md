# Validation-projection infrastructure and authoring guidance

Issue 1194 was explicitly amended by the maintainer to equivalence-before-deletion infrastructure, a migration guide and incremental slice-quality tracking. PR 1481 delivered that scope. The later architecture correction requires logic-native authoring; it does not authorize new OWL/RDFS semantic sources. Principles 4, 7, 17 and 19 apply.

Current inspection confirms both projection/migration axes, shared residue counting, the registered shape-equivalence command with nonzero failure on uncovered shapes, the canonical constraint IR and the migration guide. Ten slice and nine root legacy shape files plus structural ShEx remain; zero-legacy retirement is not claimed and remains required by issues 1197, 1446 and 1452.

## Task 1: Correct emitted migration guidance

The two slice-quality advisories still direct authors to write OWL/RDFS semantics, and one suggests a back-reference as an alternative to projection and deletion. Replace that guidance with EL-safe logic axioms or canonical logic constraints, explicit closure where required, and equivalence before deletion. Correct adjacent API comments without changing score/count semantics, finding codes or permissive legacy input recognition. Inspect the exact diff and existing score controls; normal commit hooks verify formatting and hygiene. No new runtime test is needed for this prose-only correction.

## Task 2: Publish, review and integrate

Commit and push normally, open one focused PR on the existing issue, obtain external static review, and merge only with ghprsq under the standing CI waiver. No local corpus producer, full gate, host probe, resource cap, hook bypass or extra workstream is authorized. No independent execution audit or fresh full-gate pass will be claimed. After integration, close the amended infrastructure owner with explicit remaining retirement ownership and remove its completed checkout/branch. Preserve native-ownership and GMN sibling worktrees. No duplicate logs or copied evidence archives.

Result: normal signed commit hooks and push passed. PR 1863 carries the one-file guidance correction. Exact diff review confirms score formulas, finding codes, counted inventory and legacy input recognition are unchanged. CodeRabbit completed static review with no actionable findings; there are no review threads. The integration prediction is clean against the same source base. Hosted CI remains outside the integration gate under the user waiver; no fresh full-suite or corpus pass is claimed. Integrate through ghprsq and remove only this completed checkout/branch.
