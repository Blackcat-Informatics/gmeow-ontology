# Prior art — issue #1829: fixtures: goal-directed aggregate rejected by stable persistent admission

Assembled by `stagectl brief`. Every hash and path below came from this
repository; nothing here is recalled or inferred. Read the Coverage
section at the bottom before concluding anything is absent — an empty
section can mean 'nothing found' or 'could not look'.

## Governing ADRs

The issue cites no ADRs.

## Linked issues (2-hop `#N` crawl)

- #1827: fix(build): preserve producer identity and fail docs prerequisites closed  [MERGED]  (mentions)
- #1825: build: reconcile producer input selection with compiled docs-print sources  [CLOSED]  (2-hop)
- #1826: build: stop docs synchronization when a prerequisite fails  [CLOSED]  (2-hop)

## Prior attempts at this issue

Commits citing it (including inside squash merges):

- None found.

Other branches named for it:

- None.

PRs citing it:

- #1466: chore(deps): bump typer from 0.26.7 to 0.26.8  [CLOSED]

## Related items on the forge

- #1194: V4 — Equivalence-before-deletion: retire hand-authored shapes.ttl + structural ShEx  [OPEN]
- #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path  [OPEN]
- #1295: META-EPIC: subsume Scryer/Nemo into the native physical engine (oracle-gated, fragment-by-fragment)  [CLOSED]
- #1427: logic: shared content-addressed structured-term arena for logic:/lang:/math: (hash-consed term DAG, binder-aware)  [CLOSED]
- #1307: logic: structured-term unification with function symbols + proof objects (lang:/math: grounding demand)  [CLOSED]
- #1309: pipeline: logic-layer seam cleanups (loss-ledger channel, duplicate dev compile path, compile-logic cache busting, correspondence-surface split)  [CLOSED]
- #1748: logic: a pure logic:-native corpus gets a vacuous consistency verdict — laws never reach a clash-capable surface  [OPEN]
- #1349: META-EPIC #1336 Phase 4: producer sweep, loss unification + diagnostics-slice logic:/lang:/math: grounding  [CLOSED]
- #1018: EPIC: SOTA term-page docs — full source-richness lift, Rust subsumption, curated-content migration  [CLOSED]
- #1700: perf(test): 2x required-lane optimization with DAG caching  [CLOSED]
- #1546: feat(slice-quality): ratchet gate capping OWL/shape vocabulary growth (enforce logic:-grounding, Principle 17)  [CLOSED]
- #1420: logic: engine-vs-engine external benchmark corpus + leaderboard lanes (ChaseBench, Nemo KR-2024 sets, Datalog program families)  [CLOSED]
- #504: Logic · v4 backward goals — embedded Scryer over the materialized EDB; in_world/derived_by/contradiction_witness; cut only in ProceduralPrologProfile  [CLOSED]
- #1419: logic: sorted columnar batch relations — shared-arrangement storage with amortized merge + galloping cursors  [CLOSED]

## Recurrence

Counted from `GhpRsq-Defect-Class` trailers — these are tallies, not estimates:

- `repeated-unshared-reasoning-analysis`: 2 prior instance(s)
- `compiler-input-identity-mismatch`: 1 prior instance(s)
- `consumer-artifact-path-coupling`: 1 prior instance(s)

## Coverage

What was searched, and what was not:

- issue #1829 + 0 comment(s) via github
- link crawl: 3 issue(s) within 2 hop(s) of `#N` references
- forge search on ['fixtures:', 'goal-directed', 'aggregate']: 14 other item(s)
- GhpRsq-Defect-Class trailers: 3 distinct class(es) in the last 200 commits
- prior attempts: 0 commit(s) citing #1829 (second parents walked), 0 other branch(es) named for it
- PRs citing #1829: 1
- requirement candidates: 0 line(s); 0 comment(s) matched no rule
- referenced artifacts: 0 path(s) (0 on disk), 0 identifier(s), 0 workspace package(s), 3 URL(s)
