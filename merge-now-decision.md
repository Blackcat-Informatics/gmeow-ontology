# Explicit merge-now decision

The user requested: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged."

This supersedes the previous plan to hold this branch until the complete issue contract and hosted CI are finished. It authorizes publication and integration of the existing verified increments now; it does not establish full-canon acceptance or complete issue 1770. That issue remains open and owns the remaining implementation. No code or validation gate has been weakened or bypassed. Normal ghprsq guards and hooks remain mandatory.

The source worktree is clean and all five implementation commits are already pushed. The actual current main includes the producer identity/document prerequisite repair; those build changes do not overlap this branch's source-admission/contract implementation. Git merge-tree produced a clean candidate. Existing focused evidence remains scoped as recorded in validation.md; no combined full gate is asserted.

Independent reviewer APIs are unavailable. No independent completion audit is claimed. The user explicitly requested immediate integration of this branch rather than waiting on the original completion/CI hold. This is a publication decision, not an assertion that outstanding requirements have passed.

The tracked deficiency ledger remains notice-only: unfinished original issue work is openly retained on its owning issue and is not represented as completed.
