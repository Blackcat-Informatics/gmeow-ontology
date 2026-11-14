VERDICT: PASS

## Scope
Repair of the three `helpers` failures on main, at head b442f200d. Changed:
`crates/gmeow-cli/src/commands/prove.rs` (rename of one private constant, value and single use unchanged) and
`helpers-census-debt.json` (one group member removed). No change to scripts, `Makefile` or `helpers-ledger.toml`.

## Evidence
- `make helpers-hygiene` before the change: exit 2, the three lines reproduced (`helpers-before.log`).
- The sites the `hash-domain` job counts: 46 before the rename, 45 after; the site removed is
  `crates/gmeow-cli/src/commands/prove.rs` line 51 (`hash-hits-head.txt` is the list before).
- `make helpers-hygiene` at the head, run again with the exit status recorded in the log: `EXIT=0`, no `FAIL`
  line (`helpers-primary-rerun.log`, last line).
- `cargo clippy --locked -p gmeow-cli --all-targets -- -D warnings` at the head: `EXIT=0`
  (`clippy-gmeow-cli.log`). The embedded bundle the build script needs was taken from an earlier producer pass,
  so this shows the crate compiles and lints and nothing about the bundle. This local build ran in a seeded
  build directory before a seeding fault was fixed; the hosted results below do not depend on it.
- Hosted checks at the head (run 43537767869), all finished: `helpers` success, `lint` success, `rust-prebuild`
  success, `producer-build` success. `generation (a)`: "The operation was canceled". `generation (b)`: "The
  operation was canceled". `fixture-prefix`: "The hosted runner lost communication with the server". `quality`:
  failure, the roll-up of those three.
- The same three jobs on main for the same source tree (run 43463810521 at 4c60225bb, whose tree is identical
  to main's head 05385cf3b): `generation (a)` "The hosted runner lost communication with the server",
  `generation (b)` and `fixture-prefix` "The operation was canceled", `quality` failure. On main that run's
  `helpers` job failed with the three lines this change removes.
- Independent review at b442f200d by a reader other than the author: `VERDICT: PASS`, no required finding
  (`independent-review-at-b442f200d.md`). It read the change only and ran nothing.

## Findings
- None required.
- The rename is a judgment, stated in the pull request: the constant is the authority label of a selected
  logical world, hashed as payload after that type's own separator and not itself a domain separator; four
  constants in the same role are named `AUTHORITY`. The limit this exposes in the gate (it decides by a
  constant's name and can be passed by renaming) is filed as its own required issue.
- Not run: the tests of `gmeow-cli`; the full repository gate.
- The review bot reviewed this pull request at 2026-11-14T03:14:46Z after an earlier rate-limit notice: no
  inline comment, and one finding from its linked-issues check, that the change renames the counted constant
  where the issue said to route it through the home helper or stop and report. That finding is declined on the
  substance and the departure is reported on the issue. See `review-debt.md`.

## Debt diff (helpers-census-debt.json)
```
@@ -1963,3 +1962,2 @@
     "gmeow_pipeline::stages::math_producers::MathProducersStage::default",
-    "gmeow_pipeline::stages::medium_dictionaries::MediumDictionariesStage::default",
     "gmeow_pipeline::stages::metadata::MetadataStage::default",
```
- The `Default` group shrank by the one member that no longer exists. Nothing moved, nothing grew.
- Every recorded count is unchanged; `hash-domain` stays 45.
- `helpers-ledger.toml`: no diff; no new `variant` or `[[distinct]]` row.
