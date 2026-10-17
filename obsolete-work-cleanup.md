# Superseded implementation cleanup

User instruction: "If you have a better version, don't keep the old cruft."

The native proof lookup optimization (1792) and native RDF rule migration (1805)
were already integrated through PR 1802. Both old worktrees were reconciled to
current main without reintroducing stale implementations. Current-main focused
checks passed 21 native proof cases and 10 native RDF/GTS cases. The full gate
did not pass; helpers-hygiene findings are owned by issue 1834.

Historical audit records, original staged/unstaged patches, measured benchmark
logs and current reconciliation reports were copied into reconciled-merged-work/
before cleanup. Both stagectl dry runs selected only their respective worktree
and branch. Both ordinary cleanup invocations succeeded without force.

Removed worktrees and local branches:
- 1792-case-split-class-analysis-is-quadratic
- 1805-logic-rule-projection-uses-rdf-1-1

Only the two task-specific preservation stashes were dropped after preservation:
3a51bd2228ff318197bf23285fb40055dd9e2691 and
9e8b50ed9be1f42507a4fa093b0523f425990861.
Unrelated stashes and sibling worktrees were preserved.

PR 1830 replacement e7be15f66 is pushed. The old boolean/default reasoning verdict
was removed, all callers migrated and no compatibility shim retained. Required
hooks, 43 focused tests and strict producer-independent Clippy passed. Current-head
hosted production and review are pending; this record does not claim merge-ready.
