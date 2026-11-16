# Issue #2836: The reviewed-coverage golden on main is out of date against the catalogs

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, quality

## Body

## Problem

`crates/docs/tests/fixtures/reviewed_coverage_golden.json` on `main` is out of date against the translation catalogs. Regenerating it with its own target, `make maint-refresh-reviewed-coverage-golden`, which reads only the catalogs, changes about 70 catalogs it already lists and adds about 40 it lacks. The test that compares the two, `corpus_reviewed_coverage_matches_frozen_golden`, is on the default lane and is a whole-map equality check, so it cannot pass on `main` as the file stands; no hosted run reaches the Rust test shards, so nothing has reported it.

Three open branches have each had to rewrite this single-line file for their own catalog keys, each differently, so they will conflict with one another on it.

## Acceptance

- [ ] The golden on `main` equals what its target produces from the catalogs on `main`.
- [ ] The change contains that file and nothing else.
- [ ] The open branches that touched the file merge `main` and regenerate, so each carries only its own keys.

Principles 4, 7.


## Comments (0)

