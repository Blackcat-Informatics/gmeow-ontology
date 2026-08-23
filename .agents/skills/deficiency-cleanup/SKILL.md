---
name: deficiency-cleanup
description: >-
  Drains the `.deficiencies` emergency ledger into completed, tracked remediation:
  verify every entry, consolidate duplicates, file or update the owning GitHub work,
  finish the defects, and restore the notice-only normal state. Use whenever any text
  appears below the ledger marker or when asked to clean up deficiencies.
---

# Deficiency Cleanup

`.deficiencies` is the log of last resort for critically undone work. Every entry below
its marker is **100% unauthorized**, **100% a bug**, and proof that the issue or pull
request which produced it failed. Entries record work misrepresented to pass PR gates,
work misrepresented by an agent, or a discovery that an agent was fundamentally
defective. An entry is literally a cry for help from a failing agent.

The ledger never authorizes a descope, accepted risk, carve-out, deferred phase, weaker
proxy, or merge. Its normal tracked state is the notice and marker with no entries below.
An entry blocks completion, PR creation, and merge of the work that produced it until its
defect is verified and has a durable, visible remediation owner. Removing the emergency
entry neither resolves the bug nor retroactively makes the failed work successful.

## Detect entries fail-closed

The canonical marker is `--- ENTRIES BELOW THIS LINE ---`. A missing or duplicate marker
is itself a malformed ledger and blocks the workflow. With one marker, any non-blank line
after it is an entry:

```bash
test "$(grep -cxF -- '--- ENTRIES BELOW THIS LINE ---' .deficiencies)" -eq 1
! awk '/^--- ENTRIES BELOW THIS LINE ---$/ { seen=1; next }
       seen && NF { found=1 }
       END { exit found ? 0 : 1 }' .deficiencies
```

Never append an entry to make a branch look complete. If work is incomplete, keep the
issue or PR open and report the blocker directly.

## Drain workflow

1. Read the whole ledger and split mixed blocks into concrete defects.
2. Verify every claim against current `origin/main`; stale statements are removed, not
   re-filed. Check the named code, tests, artifacts, issue state, and current behavior.
3. Inspect open issues, PRs, branches, and worktrees. Consolidate entries that share one
   landable remediation. Add evidence to an existing owner when its scope already covers
   the defect; do not create duplicates or collide with a live lane.
4. File a new `bug` only when no existing work owns the verified defect. Include current
   file/symbol evidence, the required complete outcome, negative cases, and validation.
5. Read the created or updated issue back and confirm its durable owner, evidence, and
   acceptance criteria. Only then remove the last-resort entry; filing is not resolution.
6. Remove resolved, duplicated, superseded, and now-durably-owned entry text. Leave the
   canonical notice and marker intact with nothing below it.

Do the mapping before deletion so no work is lost. Report issue creation and defect
completion as distinct states; a clean emergency ledger is not a claim that every tracked
bug is fixed.

## Repository workflow

- Use a dedicated `.worktrees/<slug>/` branch from `origin/main`; never edit the shared
  top-level checkout.
- Preserve unrelated dirty work and live lanes.
- Group by remediation owner and landable change, not by the historical PR that admitted
  the defect.
- Use normal repository labels and cite Constitution principles where applicable.
- Run `make lint` for a policy-only ledger drain. Run the complete gate required by the
  remediation itself before proposing its implementation.
- Land through the repository's protected PR workflow and `ghprsq`; never bypass it.

## Completion evidence

A successful drain proves all of the following:

- the marker exists exactly once and no non-blank entry follows it;
- every removed live defect has a verified, durable remediation owner and remains honestly
  open until completed;
- every stale or duplicate entry has current evidence explaining why it was removed;
- no remediation duplicates or conflicts with an active lane;
- required validation actually ran and passed.
