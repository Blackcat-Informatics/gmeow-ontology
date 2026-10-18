# Complete native rule encoding closure

The native runtime migration is already merged in PR 1802. Do not duplicate it. Finish the original issue by verifying its complete writer/reader/GTS contract on current main, correcting the remaining false description of literal-valued variables, and publishing the completed evidence. Principles 2, 4, 6 and 7 apply. Preserve the other worktrees and the unfinished native-proof changes.

## Task 1: Audit existing implementation and original requirements

Read the full issue and six comments. Trace native rule emission, shared typed carrier reading, scope/polarity/guard/aggregation transport and actual production-profile GTS carriage. Verify that the original implementation is already in main and classic rule scaffolding is absent. Keep foreign scoped-statement reading separate from the canonical rule grammar. Map every requirement to an existing real product control; no test may produce the ontology corpus. Review is local self-review unless a real external review is available.

## Task 2: Correct and verify the remaining description

Replace the stale aggregation-test comment that says atom variables are literals with the actual resource-carrier contract. Change no Rust tokens or runtime behavior. Reuse the same-main functional selection running in the class-performance audit workspace: native statement tests, malformed carrier and guard controls, blank resources, aggregation and actual tiny GMEOW GTS default/named-graph readback. Copy the complete result into this Stage archive after it settles. No behavioral rerun is justified by this comment-only diff. Run diff hygiene and all normal commit hooks, without bypass or profile changes.

## Task 3: Publish, review, merge and close

After the full functional contract is verified, publish a concise non-draft PR linking the original runtime merge and the exact current results. Inspect the complete review and CI surface, address actionable findings and merge only through ghprsq. Existing user authority permits pending broader CI; distinguish that from the completed issue-specific checks and never claim a full corpus gate passed. Close the original encoding defect only after the final correction is merged and its evidence is archived. Keep complete-canon and whole-pipeline requirements on their existing owning issues, not as invented prerequisites for the encoding contract.
