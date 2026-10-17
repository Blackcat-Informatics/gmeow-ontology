# Issue #1829: fixtures: goal-directed aggregate rejected by stable persistent admission

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Observed failure

Fixture-prefix job 117114464144 on PR #1827 head ee21d2c324f9331f3a6bc8c40196cccdcb2475d3 failed after optimized producer authentication passed.

Run: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38938288117

`test fixtures: produce authenticated stage DAG: stage stage-goal-directed failed: cross-process fixture requires stable persistent admission, got https://blackcatinformatics.ca/gmeow/stabilityStablePrefix / https://blackcatinformatics.ca/gmeow/cacheRecomputeAggregate`

## Required repair

Reconcile the goal-directed stage's admission/cache contract with explicit fixture production. Keep tests read-only corpus consumers; never regenerate on a test cache miss or weaken authentication. Demonstrate the explicit fixture-prefix producer and downstream consumers succeed with exact admitted identities. Principles 4, 7 and 18 apply.

Raw job evidence is retained in the PR's selected Stage archive. The user explicitly requested immediate ghprsq integration and issue ownership for CI findings; this is an unresolved failure, not a passing qualification.


## Comments (0)

