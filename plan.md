# Plan: repair the three helpers check failures on main (issue 2756)

## Behavior
`make helpers-hygiene` exits 0 on main again, and the hosted `helpers` check passes, with no recorded debt
raised and no new sanctioned group.

## Dependencies
None. Base: origin/main 05385cf3b.

## Exclusions
- The fourth helpers line (shim group 6cd0ef3c0fdc0d8d, `Stage::reads` with `empty_tiers`) belongs to the
  pipeline graph change and is fixed on its own pull request, not here.
- No change to what the helpers check enforces, to its scripts, or to `Makefile`.
- No other cleanup.

## Acceptance
- `make helpers-hygiene` exits 0 at the fixing head (log with exit status in this directory).
- `helpers-census-debt.json`: the `Default` shim group record shrinks to the group that exists; no count rises.
- The `hash-domain` count returns to 45 because the new copy uses the job's home helper; the allowance of 45 is
  unchanged.
- `helpers-ledger.toml`: no new `variant` or `[[distinct]]` row.
- Strict clippy and the focused tests of every crate whose source changed pass.
- Hosted `helpers` passes on the pull request.

## Risk
A ratchet repair is where a gate is most easily weakened. Any path that needs a raised count, a new sanctioned
group or an edit to the checking script is a stop, reported on the issue, not a fix.

## User rulings
- 2026-11-13 (session start): "You are forbidden from skipping skips, breaking trust, commiting fraud, lying
  about your work, hacking around quality gates - all things your predecessor did."
- 2026-11-13, the five merges still on main: "Review in place, fix forward (Recommended)": every finding is
  fixed through a proper PR.
- 2026-11-13, merge rule: every hosted check finished and read; review bot completed and answered; independent
  review at the merging head; no failing line that is not red on main for the same lines.
- 2026-11-13 about 21:30 local, after being told the review bot had been rate-limited and had not reviewed this
  pull request while every other condition was met: "merge 2762 wihtout coderabbit"
