# Preserve main acceptance and lower stale helper debt

Principles 4, 7 and 18. Hosted run 39401289328 built and verified the optimized
producer and passed Rust prebuild/lint, then the next main merge cancelled the
generation/fixture work. The CI workflow has cancel-in-progress true for every
branch. Preserve a running main acceptance run while newer main commits queue;
keep superseded PR runs cancellable. Retain the same concurrency groups, every
required job, both cold generations and quality requirements. Do not restart or
cancel any current run manually. No speedup or completed acceptance is claimed.

The same run's helper job failed because the Pydantic constructor now has fresh
metadata dependencies and is no longer isomorphic to the schema/slice-brief pair.
The recorded three-member shim group is stale; the two remaining members are a
strict subset of that already-recorded debt. Lower that row to the actual pair.
Do not add new exemptions, counts, groups or slack. Verify the current census
through the complete helpers-hygiene target and inspect any generated debt delta.

Run normal workflow/schema/static checks through the repository Make targets and
all normal signed hooks. This configuration/debt repair changes no Rust behavior,
so no new mirror test or whole-workspace test build is needed. No local optimized
producer, corpus producer, sync or full gate. Publish and merge through ghprsq
under the standing CI waiver, then remove the completed branch/worktree. Keep
1757 and the other full-acceptance issues open. No independent reviewer is exposed
and no independent audit is claimed.

Execution: make lint and the complete make helpers-hygiene target both passed.
The source diff is exactly the conditional cancellation expression/comment and
removal of the obsolete Pydantic member from the existing debt row. All current
groups/counts are accepted by the live census. No jobs or gate definitions changed.
No Rust tests or full producer qualification are claimed for this configuration
and debt-only unit. GitHub's current official concurrency documentation confirms
conditional cancellation and the default one-running/latest-pending behavior.

The normal signed commit and all configured hooks passed. The merge-tree
prediction is clean against fetched main, with no concurrent base source delta.

Final review: no inline findings or submitted reviews at inspection. CI is running and automatic review is pending; integration follows the explicit CI waiver. Source is clean and pushed, with clean merge prediction against the fetched base. No independent audit or full corpus acceptance is claimed.
