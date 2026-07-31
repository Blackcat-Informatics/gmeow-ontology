---
name: deficiency-cleanup
description: >-
  Drains the local `.deficiencies` ledger into tracked GitHub work — triage every block,
  delete what is operational, resolved or duplicated, file the live shortfalls, and leave
  the file empty. Use when the ledger has accumulated entries, or when asked to convert
  `.deficiencies` into issues.
---

# Deficiency Cleanup — draining the ledger into tracked work

`.deficiencies` is the append-only receipt log for descopes a human authorised. It is
**not** a backlog, and nothing in this repository reads it — no test, no lint, no CI job,
no Makefile target. Its own doctrine says so plainly: there is no sweep, entries sit
indefinitely, and an entry is *a headstone, not a ticket*.

This skill is the sweep that doctrine says does not exist. Run it and the ledger becomes
what it should be: empty, with every live shortfall tracked where a human will see it.

> **Meta-rule:** the controlling doctrine is the `_record-deficiencies` include used by the
> stage-3 flow, plus the `.deficiencies` paragraph in `CLAUDE.md`. Read the include before
> you start — this skill implements it, it does not replace it. If the two conflict, the
> include wins.

---

## 1. When to run

- When asked to clean up, drain, or convert `.deficiencies`.
- When the ledger has grown past a handful of blocks. The doctrine's own tripwire is that
  **three entries for one issue means that issue was not implemented** — so a ledger with
  dozens of blocks is a signal in its own right, not just clutter.

Do **not** run this as part of a feature branch. It is its own change, on its own branch,
touching only the ledger. Mixing it into feature work hides both.

---

## 2. Start with the validity test

Before reading a single block, count:

```bash
grep -cE '^(##|====|────)' .deficiencies   # blocks
grep -c 'Decided-by:' .deficiencies        # valid entries
```

The doctrine requires every entry to carry a real `Decided-by:` line naming the human who
authorised the descope and quoting their answer, and it states that **an entry without one
is invalid — delete it**. If the two counts diverge sharply, most of the file is already
formally deletable and the triage below is about salvaging the *content*, not defending the
*entries*.

A second standing deletion rule matters just as much: **a deferral that was later resolved
leaves nothing to record.** Interim notes get deleted, not narrated.

---

## 3. Triage every block

Read the whole file. For each block, and often for each bullet within a block, assign
exactly one outcome.

| Outcome | What it looks like |
|---|---|
| **DELETE — operational** | Machine load, wall-clock budget, a killed process, a full disk, "delegated to CI", "deferred to a later stage", "ran green later", "aborted to protect the worktree". The doctrine bans all of these by name. A gate that could not be run is unfinished work, never an entry. |
| **DELETE — resolved** | The same file says "RESOLVED", "VOID", "superseded", or a later block reverses an earlier one. Also: the shortfall was fixed by later work elsewhere (verify — see step 4). |
| **DELETE — duplicate** | Two blocks record the same shortfall, usually a stage-2 entry and a stage-3 entry for the same issue. Keep the later, more accurate one for triage; delete both once filed. |
| **DELETE — not a deficiency** | A design boundary with no forward path: a mathematical ceiling, an upstream library's deliberate shape, a behaviour that is correct as shipped. If the block itself says "no forward path" or "correct behaviour", believe it. |
| **KEEP for filing** | A live shortfall in the shipped code or artifacts, with a real forward path. Usually flagged by a `FORWARD PATH:` line. |

Two traps:

- **A block is not an atom.** A single block often mixes an operational complaint, a
  resolved note, and one genuine shortfall. Triage bullets, not headers.
- **Self-justification reads like content.** Paragraphs explaining why something was not
  done, how careful the author was, or what the adversary concluded are process narration.
  Extract the code fact; discard the defence.

---

## 4. Verify every survivor against current `main` — before filing

**Never file a claim you did not re-check.** Ledger entries go stale silently, and this is
the single highest-value step in the skill: a drain that skips it files issues for work
that is already done, which is worse than not draining at all.

For each survivor, find the code and confirm the condition still holds. Cheap checks that
repeatedly matter here:

```bash
git ls-files <path>                 # is that artifact even tracked any more?
ls <dir>                            # does the vendored tree / override still exist?
grep -rn "<the symbol named in the entry>" crates/ slices/
gh issue view <n> --json state      # did the issue the entry points at already close?
```

Also re-read `.config/nextest.toml`, `.pre-commit-config.yaml` and the root manifests
directly — several entries describe gate placement and dependency pins, which move often.

Expect a meaningful fraction to have resolved themselves. Delete those; do not file them.

---

## 5. Check what is in flight — never file into a live lane

```bash
git worktree list
gh pr list --state open --json number,title,headRefName,isDraft
for w in .worktrees/*/; do
  git -C "$w" diff --name-only origin/main...HEAD | sed 's|/[^/]*$||' | sort -u
done
```

- A survivor whose files a live branch is already editing gets a **comment on that issue or
  PR**, never a new issue. Filing into someone's open lane duplicates their work and invites
  a scope fight at review.
- A worktree with **zero commits against `origin/main`** is inactive — it does not count as
  in flight.
- Note any branch that itself appends to `.deficiencies`: emptying the file will conflict
  there on its next merge from `main`. Say so on that PR rather than letting it surprise
  them mid-merge.

---

## 6. Group, then map onto the open issue list

Read the open issues in full before writing a single new one.

```bash
gh issue list --state open --limit 200 --json number,title,labels,body
```

Then, in order:

1. **Group survivors by subsystem and by the work that would close them.** Several ledger
   entries recorded from different branches routinely turn out to be one piece of work.
2. **Give existing issues what they already own.** If an open issue's scope covers a
   survivor, comment on it — that is tracking, and it is cheaper for everyone than a new
   issue that has to be triaged and closed as a duplicate.
3. **Only then file what is left.** Aim for issues that are each one landable change. An
   issue nobody can start without decomposing it first has deferred the decomposition, not
   removed it — the work-sizing section of `AGENTS.md` is explicit about this.

Follow house practice on labels: one kind (`bug` / `enhancement` / `documentation`), one
complexity (`complexity-easy` / `-medium` / `-hard`), plus scope (`cross-cutting`,
`foundational`, `epic`), any touched slice domain (`logic`, `normative`, `narrative`, …),
`rust` if a crate changes, and `quality` for a gate or correctness invariant. Express
dependencies as `Blocked by:` / `Blocks:` lines in the body. Milestones are not used.

Each issue body should carry: where it came from (the ledger, and which branch recorded
it), the **verified** current-state evidence with file and line references, the forward
path the entry named, and an acceptance criterion. An issue that just restates the ledger
prose is not an improvement on the ledger.

---

## 7. File, comment, then delete what was filed

All three, in that order, in one pass.

- Deleting without filing loses the work.
- Filing without deleting leaves the ledger unread and still growing — and the next drain
  has to re-triage everything.

Rewrite `.deficiencies` to a header stating the drain date, the triage categories, and
where the work went. Keep the format template in the header so the next author has the
`Decided-by:` contract in front of them.

---

## 8. Landing

- Work in `.worktrees/<slug>/` on a branch off `origin/main` — never in the top-level
  checkout, which a daemon resets to clean `main`.
- Conventional-commit subject with a scope, e.g. `chore(deficiencies): …`.
- **The issue-reference lint applies to this file.** `scripts/lint-issue-refs.sh` is
  CI-blocking and rejects a hash followed by three or more digits in *any* Markdown outside
  a small exclusion list — which includes this skill. Refer to issues by title or
  description here. `.deficiencies` itself is exempt (it is not Markdown), so the ledger
  header may cite numbers freely.
- Verify with `make lint` (pre-commit runs the reference lint on every invocation). A drain
  touches no Rust, no ontology source and no pipeline input, so the regeneration pipeline
  has nothing to produce — do not run `make check`.

---

## 9. What a good outcome looks like

- `.deficiencies` is a header and nothing else.
- Every deleted block is deleted for a reason you can name from the table in section 3.
- Every filed issue carries evidence you re-verified yourself, not prose you copied.
- No new issue overlaps an open one or an in-flight branch.
- The completion report says plainly what was filed and what was thrown away — the ledger
  being empty is the claim, and it has to be true.
