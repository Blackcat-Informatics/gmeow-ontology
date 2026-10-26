# Issue #1961: perf(pipeline): qualify and split independent conformance recorder scheduling

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, rust, complexity-hard

## Body

Blocked by: none.
Blocks: #1962 and #1757.

Assess the existing independent stage-conformance recorder families and remove serial scheduling only where actual measurement establishes value. The historical main run 39742980452 records 213,354 ms for stage-conformance. Preserve the complete selected recorder inventory and output identities.

Acceptance:

1. Read current recorder dependencies and available timings before changing scheduling. Separate independent families into deterministic branches when supported by that evidence; otherwise establish the concrete dependency or current parallel ownership that prevents a useful split.
2. Merge isolated artifact maps in declaration order and hard-fail duplicate ownership. Preserve required outputs, loss evidence and source/context identities.
3. Qualify scheduling and duplicate-ownership behavior using authored component inputs, scoped production caller compilation and normal hooks. No test constructs corpus artifacts. Review the source change independently and land through ghprsq.
4. Keep authenticated producer settings and the complete required build surface unchanged. Full production output parity and comparative elapsed/RSS/intermediate-byte evidence are owned by the final qualification child.

Principles 4, 7, 12 and 18. This child owns recorder scheduling; it does not claim a full gate or close #1757.


## Comments (2)

### paudley — 2026-10-26T10:36:57Z

Current-source and historical-run review found no justified additional parallel split yet. In main run 39742980452, conformance completed in 213,354 ms and finished more than 36 minutes before reasoning. Shortening conformance alone would not shorten that observed run. Execution/native cases and supplemental sweeps already parallelize internally; the top-level families remain serial.

The required chains are execution through admissions and grading, math lowering through source lint, and operator preparation through grounding and scenes. The current shared artifact map does not provide isolated merging or duplicate-owner refusal. Those invariants must be added if measurements justify a split.

The next concrete step is per-recorder timing evidence in the next authorized hosted producer, followed by reassessment against the reasoning change. This is an evidence gap, not a performance pass or authorization to run a local producer. The child remains open.


### paudley — 2026-10-26T16:37:38Z

The hosted measurement change completed all 36 selected operations in generation A of run 40317325879. Their elapsed times total 151,471 ms: execution 93,977 ms, math lowering 22,078 ms, GMN grounding 12,362 ms, signatures 4,509 ms, supplemental 3,430 ms, source lint 3,328 ms, verify gates 2,800 ms and smaller operations for the remainder. These are observations from one run, not a before/after speedup.

Independent source assessment confirms execution already overlaps native observations with compilation and parallelizes case misses. However, math lowering followed by source lint is a separate approximately 25.4-second chain. Its returned observations map is the only handoff; execution admissions do not block that chain. Thus the no-useful-split exception is not established. This issue stays open.

The bounded scheduling candidate is that one chain, retaining its sequential dependency and deterministic artifact-map merge with duplicate-path refusal and stable error priority. Qualification must compare downstream readiness and total generation as well as recorder time because the shared pool can introduce contention. No broad family fanout or speedup claim is justified yet.

Conformance completed before snapshot assembly; the run later received a runner shutdown signal during snapshot canonicalization. Full generation, parity and consumer acceptance remain open under #1962. The separately observed stack-pressure repair is owned by #1966.


