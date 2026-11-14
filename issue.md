# Issue #2756: Repair the three helpers check failures on main without raising any debt

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

The hosted `helpers` check passed on `main` on 2026-11-12 (run 43263534387) and has failed on every run since.
Three failing lines are on `main` today. Each came in with a change that merged before its checks had finished.

## The three lines

From `make helpers-hygiene` (`scripts/check-shared-helpers.py`), run 43463810521:

```
FAIL: isomorphic shim group shim:e7b4e9dec554ea70 is sanctioned by no variant or [[distinct]] row: gmeow_foundation_corpus::importer::FoundationImporter::default, ...
FAIL: recorded debt shim group [... medium_dictionaries::MediumDictionariesStage::default ...] no longer exists as recorded; lower the debt with `scripts/check-shared-helpers.py --write-debt`
FAIL: job `hash-domain`: 46 copies outside its home, debt allows 45
```

- The first two appear at 56545ecd1 (#2737). That change removed `impl Default for MediumDictionariesStage`, so
  the recorded group of `Default` implementations lost a member and no longer matches its record.
- The third appears first in the run of #2746 and is absent from the run of its base. One new copy of the
  hash-domain job entered with that change. The script names no file. The likeliest site is the new `sha256_hex`
  in `crates/gmeow-cli/src/commands.rs`; this is not yet confirmed.

## Wanted

1. The recorded `Default` group matches the group that exists: the record shrinks to the smaller group. No
   group is added and no count rises.
2. The new hash-domain copy is found and routed through the job's home helper, so the count returns to 45. The
   allowance is not raised.
3. If either cannot be done without raising a recorded debt, stop and say so on this issue.

## Acceptance

- `make helpers-hygiene` exits 0 locally at the fixing head.
- The hosted `helpers` check passes on the fixing pull request and on `main` after the merge.
- `helpers-census-debt.json` and `helpers-ledger.toml` show no raised count and no new sanctioned group.


## Comments (0)

