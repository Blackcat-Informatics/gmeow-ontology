Three stale nextest selectors break the lane-reference contract. Remove the two retired exclusions and their rationale, and point the GMN budget at the current independent-user-corpus test. Every live exclusion retains its paired justification; no live test or budget is removed.

Acceptance could also report 100% recall when its corpus contained no addressable triples. Reject that absent measurement even with a zero development floor. Keep the default floor at 64.5%, document explicit overrides accurately, and distinguish the CI acceptance matrix branch from the Make aggregate. Principles 4, 7 and 18 apply.

Validation: nine focused native contracts passed, including an actual per-file round-trip gate over OWL-only input and cold configuration parsing. Full make helpers-hygiene, strict producer-independent all-target Clippy and all normal commit hooks passed on the final source. CodeRabbit reviewed the original delta; all three findings are addressed, and both inline threads have explicit acknowledgments. No local corpus producer or external-corpus measurement was run. Full pipeline qualification remains pending hosted CI.

Closes #1644.
