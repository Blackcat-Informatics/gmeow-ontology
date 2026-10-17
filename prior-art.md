# Prior art — issue #1803: Split gmeow/purrdf responsibilities: generic engines in purrdf, logic: semantics in gmeow

Assembled by `stagectl brief`. Every hash and path below came from this
repository; nothing here is recalled or inferred. Read the Coverage
section at the bottom before concluding anything is absent — an empty
section can mean 'nothing found' or 'could not look'.

## Governing ADRs

The issue cites no ADRs.

## Linked issues (2-hop `#N` crawl)

- #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path  [OPEN]  (mentions)
- #1802: fix(logic): integrate PurRDF main and production reasoning repairs  [MERGED]  (mentions)
- #1796: Two authored existential cycles through inverse properties cannot be certified by a skolem-chase termination certifier  [OPEN]  (mentions)
- #1797: Lift the chase termination ladder and Datalog-first scheduling into purrdf-datalog, then consume it  [OPEN]  (mentions)
- #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed  [MERGED]  (mentions)
- #1805: logic: rule projection uses RDF 1.1 classic reification (rdf:Statement); move to RDF 1.2 triple terms  [OPEN]  (mentions)
- #451: [TEMPLATE] Up-projection per slice/segment (instantiate after the audit)  [CLOSED]  (mentions)
- #453: gmeow transpile entry point + single-vocab down-projection mode  [CLOSED]  (mentions)
- #452: Down-projection hardening: co-typing, fan-out, BCP-47 retag, consumer-clean tiers  [CLOSED]  (mentions)
- #1803: Split gmeow/purrdf responsibilities: generic engines in purrdf, logic: semantics in gmeow  [OPEN]  (mentions)
- #1761: fix: classify extracted test modules outside the production repo-static census  [OPEN]  (mentions)
- #1762: refactor: complete Diag migration for prover and physical execution errors  [OPEN]  (mentions)
- #1764: test: keep slicetest synthetic inventory corpus-free  [CLOSED]  (mentions)
- #1840: fix(logic): preserve native refusals and correct music data roles  [MERGED]  (mentions)
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
- #1770: feat: certify and precompile the complete canon for consumer proof  [OPEN]  (2-hop)
- #1768: feat: add a first-class consumer prover entry point  [OPEN]  (2-hop)
- #1791: perf(1757): make native reasoning fit and finish on the production corpus  [MERGED]  (2-hop)
- #1789: Chase admission skips input-specific termination certification at production scale (4096-fact evidence cap)  [CLOSED]  (2-hop)
- #448: EPIC — full transpile: source RDF → pure-GMEOW draft → maximal multi-vocab output (adoption)  [CLOSED]  (2-hop)
- #1: docs: community-health files + dependabot config  [MERGED]  (2-hop)
- #4: build(deps): bump actions/upload-artifact from 4 to 7  [MERGED]  (2-hop)
- #2: build(deps): bump astral-sh/setup-uv from 5 to 8.1.0  [MERGED]  (2-hop)
- #1336: META-EPIC: unified diagnostic substrate — carrier-borne, RDF-context-bound graded witnesses for errors, warnings, advice, and events  [OPEN]  (2-hop)

## Prior attempts at this issue

Commits citing it (including inside squash merges):

- `28d77401b fix(logic): preserve native refusals and correct music data roles`
- `9fa28c6fe fix(logic): keep rejected decimals outside rational interpretation`
- `816a604cd fix(logic): prepare exact questions for native proof checking`
- `10c6e175f refactor(logic): retain native DL certificates and proofs`

Other branches named for it:

- None.

PRs citing it:

- #1837: fix(logic): preserve native evidence and portable reasoning identities  [MERGED]  (this stage's own PR)
- #1840: fix(logic): preserve native refusals and correct music data roles  [MERGED]  (this stage's own PR)
- #1838: refactor(logic): delegate anyURI semantics to PurRDF  [MERGED]  (this stage's own PR)
- #1839: fix(logic): preserve native datatype lexical admission  [MERGED]  (this stage's own PR)
- #1832: feat(logic): preserve canon admission and native proof evidence  [MERGED]

## Related items on the forge

None found by title-term search.

## Recurrence

Counted from `GhpRsq-Defect-Class` trailers — these are tallies, not estimates:

- `none`: 8 prior instance(s)
- `repeated-unshared-reasoning-analysis`: 2 prior instance(s)
- `https//blackcatinformatics.ca/gmeow/RecordedConsumerFailure`: 1 prior instance(s)
- `compiler-input-identity-mismatch`: 1 prior instance(s)
- `consumer-artifact-path-coupling`: 1 prior instance(s)

## Coverage

What was searched, and what was not:

- issue #1803 + 22 comment(s) via github
- link crawl: 34 issue(s) within 2 hop(s) of `#N` references
- forge search on ['Split', 'gmeow/purrdf', 'responsibilities:']: 0 other item(s)
- GhpRsq-Defect-Class trailers: 5 distinct class(es) in the last 200 commits
- prior attempts: 4 commit(s) citing #1803 (second parents walked), 0 other branch(es) named for it
- PRs citing #1803: 5
- requirement candidates: 31 line(s); 7 comment(s) matched no rule
- referenced artifacts: 21 path(s) (12 on disk), 26 identifier(s), 2 workspace package(s), 3 URL(s)
