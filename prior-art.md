# Prior art — issue #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path

Assembled by `stagectl brief`. Every hash and path below came from this
repository; nothing here is recalled or inferred. Read the Coverage
section at the bottom before concluding anything is absent — an empty
section can mean 'nothing found' or 'could not look'.

## Governing ADRs

The issue cites no ADRs.

## Linked issues (2-hop `#N` crawl)

- #1740: perf(logic): integrate the authenticated native pipeline and calculus  [MERGED]  (mentions)
- #1704: Agentic modal kernel epic: intent algebra, deontic work graphs, choreography-as-projection, LTLf temporal operators  [OPEN]  (mentions)
- #1748: logic: a pure logic:-native corpus gets a vacuous consistency verdict — laws never reach a clash-capable surface  [OPEN]  (mentions)
- #317: Music · Timbre & the sensory bridge (MIR observations, embeddings by reference)  [CLOSED]  (mentions)
- #1760: perf(logic): restore certified batch slice probes  [MERGED]  (mentions)
- #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path  [OPEN]  (mentions)
- #1758: fix(ci): admit transferred optimized producers  [MERGED]  (mentions)
- #1759: perf(pipeline): schedule stages on completion  [MERGED]  (mentions)
- #1732: perf: persist completed producer fixtures across CI runs  [CLOSED]  (2-hop)
- #1695: bug(logic): route modal verdict evaluation through the shipped reasoner  [CLOSED]  (2-hop)
- #1731: build: authenticate an O3 full-LTO pipeline producer  [CLOSED]  (2-hop)
- #1739: build: authenticate the O3/full-LTO pipeline producer  [MERGED]  (2-hop)
- #1733: logic: implement the composite agentic modal kernel  [OPEN]  (2-hop)
- #1734: logic: evaluate LTLf operators over enactment journals  [OPEN]  (2-hop)
- #1735: logic: derive per-step deontic work-graph assessments  [OPEN]  (2-hop)
- #1747: logic: reachability discharge (Arm A) never consults the corpus's own compiled rule heads  [OPEN]  (2-hop)
- #1736: logic: implement compositional intent algebra and revision  [OPEN]  (2-hop)
- #1737: logic: project certified protocol roles into orchestrations  [OPEN]  (2-hop)
- #1742: logic: implement correspondence composition and standpoint-indexed merge  [OPEN]  (2-hop)
- #1743: perf: certify correspondence normalization and physical fusion  [OPEN]  (2-hop)
- #1738: perf: execute the production pipeline as a certified deontic work graph  [OPEN]  (2-hop)
- #1745: perf: adopt typed correspondence execution throughout the production pipeline  [OPEN]  (2-hop)
- #272: feat(gts): COSE signing & encryption — evidence/opaque profiles + remaining §18 vectors  [CLOSED]  (2-hop)
- #1741: logic: complete typed correspondence execution and law-bearing source recovery  [OPEN]  (2-hop)
- #1755: [bug] Compact LogicAxiom lowering collapses distinct RDF literal identities  [OPEN]  (2-hop)
- #1744: logic: complete correspondence targets and executable openEHR exemplars  [OPEN]  (2-hop)
- #1749: slicetest: competency cells with cqDataFile cannot use a reasoning lane, and always merge the repo's own store  [OPEN]  (2-hop)
- #1712: logic/mappings: salvage the correspondence frontend — dsl/mappings cells as logic:Correspondence IR  [OPEN]  (2-hop)
- #1746: logic: non-entailment obligation discharge misses logic:instanceOf-typed obligations  [OPEN]  (2-hop)
- #1750: cli: consistency-family commands need gate-able exit codes and machine-readable verdicts  [OPEN]  (2-hop)
- #1751: logic: TPTP-FOF export + external prover harness (whole-corpus FOL verdicts)  [OPEN]  (2-hop)
- #1752: slice-quality: an external-corpus mode for sacred/consumer slices outside the GMEOW checkout  [OPEN]  (2-hop)
- #306: EPIC: Music — frame-relative musical content; every notation a lossy projection  [CLOSED]  (2-hop)
- #307: Music · Structural foundation (extension scaffold, WEMI specialization, derivation & genre)  [CLOSED]  (2-hop)
- #310: Music · Musical time (time frames, tempo maps, meter, polymeter vs polyrhythm, groove)  [CLOSED]  (2-hop)

## Prior attempts at this issue

Commits citing it (including inside squash merges):

- `28817b370 fix(logic): preserve alternate calculus spellings`

Other branches named for it:

- `issue-1757-ja-linear`
- `issue-1757-snapshot-critical`
- `origin/issue-1757-ja-linear`
- `origin/issue-1757-production-verdict`
- `origin/issue-1757-snapshot-critical`

PRs citing it:

- #1801: perf(1757): compute joint acyclicity in linear Horn propagation  [OPEN]
- #1802: fix(1757): certify and run native reasoning on the production corpus  [OPEN]
- #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed  [OPEN]
- #1791: perf(1757): make native reasoning fit and finish on the production corpus  [MERGED]
- #1785: feat(dev): helper census: one home per job, PurRDF's where it exists  [MERGED]
- #1758: fix(ci): admit transferred optimized producers  [MERGED]
- #1760: perf(logic): restore certified batch slice probes  [MERGED]
- #1759: perf(pipeline): schedule stages on completion  [MERGED]
- #1740: perf(logic): integrate the authenticated native pipeline and calculus  [MERGED]
- #1788: fix(1787): carry every logic: list to graph/logic in one encoding  [MERGED]
- #1784: deps: adopt purrdf 3.0 and enforce a major-3 substrate floor  [MERGED]

## Related items on the forge

- #1745: perf: adopt typed correspondence execution throughout the production pipeline  [OPEN]
- #1194: V4 — Equivalence-before-deletion: retire hand-authored shapes.ttl + structural ShEx  [OPEN]
- #1741: logic: complete typed correspondence execution and law-bearing source recovery  [OPEN]
- #1518: perf(rust): optimize the native pipeline and make check critical path  [CLOSED]
- #1655: repo hygiene: sweep stale process references and tighten the ref lint  [CLOSED]
- #1700: perf(test): 2x required-lane optimization with DAG caching  [CLOSED]
- #629: EPIC — Remove rdflib repo-wide: port remaining graph logic to Rust + oxigraph (gmeow_validate), pyoxigraph-only Python seam  [CLOSED]
- #856: logic-compile: have gmeow_logic.compile_logic return a native Report directly (drop the Python dict forward)  [CLOSED]
- #840: P6 — purrdf: RdfEventSource/RdfEventSink seam  [CLOSED]
- #1330: Upgrade to purrdf 0.3.0 and migrate slice tests to the bnode-capable Rust harness  [CLOSED]
- #835: P1 — purrdf: SRP-decompose py_store.rs  [CLOSED]
- #713: T3 — Hypothetical / subjunctive operator (◇): test without committing  [CLOSED]
- #710: epic: Transaction Logic (Bonner–Kifer) — state-change, queries+updates+transactions unified in the logic: layer  [CLOSED]
- #1413: feat(logic): decide the withheld OWL 2 DL/Full fragment natively — complement refutation, qualified-cardinality counting, nominal/datatype value-space, xsd facet evaluation  [CLOSED]

## Recurrence

Counted from `GhpRsq-Defect-Class` trailers — these are tallies, not estimates:

- `consumer-artifact-path-coupling`: 1 prior instance(s)
- `repeated-unshared-reasoning-analysis`: 1 prior instance(s)

## Coverage

What was searched, and what was not:

- issue #1757 + 24 comment(s) via github
- link crawl: 35 issue(s) within 2 hop(s) of `#N` references
- forge search on ['perf/logic:', 'finish', 'merged']: 14 other item(s)
- GhpRsq-Defect-Class trailers: 2 distinct class(es) in the last 200 commits
- prior attempts: 1 commit(s) citing #1757 (second parents walked), 5 other branch(es) named for it
- PRs citing #1757: 11
- requirement candidates: 19 line(s); 14 comment(s) matched no rule
- referenced artifacts: 15 path(s) (12 on disk), 24 identifier(s), 3 workspace package(s), 0 URL(s)
