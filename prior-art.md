# Prior art — issue #1762: refactor: complete Diag migration for prover and physical execution errors

Assembled by `stagectl brief`. Every hash and path below came from this
repository; nothing here is recalled or inferred. Read the Coverage
section at the bottom before concluding anything is absent — an empty
section can mean 'nothing found' or 'could not look'.

## Governing ADRs

The issue cites no ADRs.

## Linked issues (2-hop `#N` crawl)

- #1336: META-EPIC: unified diagnostic substrate — carrier-borne, RDF-context-bound graded witnesses for errors, warnings, advice, and events  [OPEN]  (part of)
- #1762: refactor: complete Diag migration for prover and physical execution errors  [OPEN]  (mentions)
- #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path  [OPEN]  (mentions)
- #1802: fix(logic): integrate PurRDF main and production reasoning repairs  [MERGED]  (mentions)
- #1310: diagnostics: finding-attached remediation (repair suggestions) + explanation-backed feedback for end users  [CLOSED]  (2-hop)
- #1330: Upgrade to purrdf 0.3.0 and migrate slice tests to the bnode-capable Rust harness  [CLOSED]  (2-hop)
- #1314: meta: remove remaining Python from primary use cases and migrate test suite to Rust  [CLOSED]  (2-hop)
- #1740: perf(logic): integrate the authenticated native pipeline and calculus  [MERGED]  (2-hop)
- #1704: Agentic modal kernel epic: intent algebra, deontic work graphs, choreography-as-projection, LTLf temporal operators  [OPEN]  (2-hop)
- #1748: logic: a pure logic:-native corpus gets a vacuous consistency verdict — laws never reach a clash-capable surface  [OPEN]  (2-hop)
- #317: Music · Timbre & the sensory bridge (MIR observations, embeddings by reference)  [CLOSED]  (2-hop)
- #1760: perf(logic): restore certified batch slice probes  [MERGED]  (2-hop)
- #1793: fix(1789): specialize termination analysis without combinatorial products  [MERGED]  (2-hop)
- #1794: fix(1790): publish each world's own chase certificate and enforce the demonstrator classes  [CLOSED]  (2-hop)
- #1798: feat(1796): run the restricted chase Datalog-first  [MERGED]  (2-hop)
- #1799: feat(1796): certify restricted joint acyclicity  [MERGED]  (2-hop)
- #1800: fix(1795): sharpen termination-statement encoding and bind closed relations  [MERGED]  (2-hop)
- #1801: perf(1757): compute joint acyclicity in linear Horn propagation  [MERGED]  (2-hop)
- #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed  [MERGED]  (2-hop)
- #1770: feat: certify and precompile the complete canon for consumer proof  [OPEN]  (2-hop)
- #1768: feat: add a first-class consumer prover entry point  [OPEN]  (2-hop)

## Prior attempts at this issue

Commits citing it (including inside squash merges):

- `f033b9f3a docs(logic): clarify diagnostic contracts and native terminology`
- `12380e2df fix(logic): remove facet string errors and ground service refusals`

Other branches named for it:

- `issue-1762-diag-migration`
- `origin/issue-1762-diag-migration`

PRs citing it:

- #1837: fix(logic): preserve native evidence and portable reasoning identities  [MERGED]
- #1835: fix(logic): unify helper ownership and typed diagnostic failures  [MERGED]
- #1763: perf: reuse authenticated validation state  [MERGED]
- #1832: feat(logic): preserve canon admission and native proof evidence  [MERGED]

## Related items on the forge

- #1803: Split gmeow/purrdf responsibilities: generic engines in purrdf, logic: semantics in gmeow  [OPEN]
- #1194: V4 — Equivalence-before-deletion: retire hand-authored shapes.ttl + structural ShEx  [OPEN]
- #1645: deps/purrdf: retire the rev pin and the dead cutover scaffolding  [CLOSED]
- #743: H9 — slice-quality: prose↔axiom harvest-coverage axis + coverage report (per-slice harvest sweep → background uplift lane)  [CLOSED]
- #287: refactor(repo): re-layout — make the canonical/generated/ephemeral boundary the filesystem boundary (single generated/ root, dsl/ grouping, .build/ staging, src/ subpackages)  [CLOSED]
- #731: P4 — Retire logic_adapter.py in lockstep with #694 (legacy owl/gUFO isomorphism oracle)  [CLOSED]
- #1551: Migrate math slice (and repo) to logic:-only grounding; retire sh:/owl: authoring + scope slicetest shapes per-slice  [CLOSED]
- #1579: Projection fidelity: lower owl:FunctionalProperty → sh:maxCount 1 in derived SHACL/Python (+ oracle cardinality-loss guard)  [CLOSED]
- #1556: grounding/logic: close correspondence authority and correct external bridge semantics  [CLOSED]
- #1555: grounding: complete shipped external grounding for logic:, math:, and lang:  [CLOSED]
- #1365: [feature] Migrate the graph-API traversal test cluster to native Rust conformance  [CLOSED]
- #1366: [feature] Migrate the mappings/SSSOM test cluster to native Rust; retire mappings.py  [CLOSED]
- #1367: [feature] Migrate the reasoning/OWL-RL test cluster to ontology_entailments.rs  [CLOSED]
- #1200: Lift SKOS (annotations + skos:*Match) into logic: and project SKOS back (close the isSupersetOf gap)  [CLOSED]

## Recurrence

Counted from `GhpRsq-Defect-Class` trailers — these are tallies, not estimates:

- `none`: 11 prior instance(s)
- `missing-intrinsic-runtime`: 2 prior instance(s)
- `repeated-unshared-reasoning-analysis`: 2 prior instance(s)
- `https//blackcatinformatics.ca/gmeow/RecordedConsumerFailure`: 1 prior instance(s)
- `compiler-input-identity-mismatch`: 1 prior instance(s)
- `consumer-artifact-path-coupling`: 1 prior instance(s)

## Coverage

What was searched, and what was not:

- issue #1762 + 5 comment(s) via github
- link crawl: 21 issue(s) within 2 hop(s) of `#N` references
- forge search on ['refactor:', 'complete', 'migration']: 14 other item(s)
- GhpRsq-Defect-Class trailers: 6 distinct class(es) in the last 200 commits
- prior attempts: 2 commit(s) citing #1762 (second parents walked), 2 other branch(es) named for it
- PRs citing #1762: 4
- requirement candidates: 9 line(s); 2 comment(s) matched no rule
- referenced artifacts: 6 path(s) (6 on disk), 8 identifier(s), 4 workspace package(s), 0 URL(s)
