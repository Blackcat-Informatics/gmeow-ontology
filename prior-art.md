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
- #1803: Split gmeow/purrdf responsibilities: generic engines in purrdf, logic: semantics in gmeow  [OPEN]  (mentions)
- #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed  [MERGED]  (mentions)
- #1802: fix(logic): integrate PurRDF main and production reasoning repairs  [MERGED]  (mentions)
- #1782: fix: reason::artifacts closure test fixture lacks native execution evidence (red on main)  [OPEN]  (mentions)
- #1805: logic: rule projection uses RDF 1.1 classic reification (rdf:Statement); move to RDF 1.2 triple terms  [CLOSED]  (mentions)
- #1838: refactor(logic): delegate anyURI semantics to PurRDF  [MERGED]  (mentions)
- #1783: fix: make clippy fails on main (build-inputs, action-cache, logic-compile lints)  [OPEN]  (mentions)
- #1876: Use validated ownership for model export documentation  [MERGED]  (mentions)
- #1807: [term] Literal typing: language on words, types for machinery, time at its stated precision (consumers: memetics, katamari)  [OPEN]  (mentions)
- #1732: perf: persist completed producer fixtures across CI runs  [CLOSED]  (2-hop)
- #1695: bug(logic): route modal verdict evaluation through the shipped reasoner  [CLOSED]  (2-hop)
- #1731: build: authenticate an O3 full-LTO pipeline producer  [CLOSED]  (2-hop)
- #1739: build: authenticate the O3/full-LTO pipeline producer  [MERGED]  (2-hop)
- #1733: logic: implement the composite agentic modal kernel  [OPEN]  (2-hop)
- #1734: logic: evaluate LTLf operators over enactment journals  [OPEN]  (2-hop)
- #1735: logic: derive per-step deontic work-graph assessments  [OPEN]  (2-hop)
- #1747: logic: reachability discharge (Arm A) never consults the corpus's own compiled rule heads  [CLOSED]  (2-hop)
- #1736: logic: implement compositional intent algebra and revision  [OPEN]  (2-hop)
- #1737: logic: project certified protocol roles into orchestrations  [OPEN]  (2-hop)
- #1742: logic: implement correspondence composition and standpoint-indexed merge  [OPEN]  (2-hop)
- #1743: perf: certify correspondence normalization and physical fusion  [OPEN]  (2-hop)
- #1738: perf: execute the production pipeline as a certified deontic work graph  [OPEN]  (2-hop)
- #1745: perf: adopt typed correspondence execution throughout the production pipeline  [OPEN]  (2-hop)
- #272: feat(gts): COSE signing & encryption — evidence/opaque profiles + remaining §18 vectors  [CLOSED]  (2-hop)
- #1741: logic: complete typed correspondence execution and law-bearing source recovery  [OPEN]  (2-hop)
- #1755: [bug] Compact LogicAxiom lowering collapses distinct RDF literal identities  [CLOSED]  (2-hop)
- #1744: logic: complete correspondence targets and executable openEHR exemplars  [OPEN]  (2-hop)
- #1749: slicetest: competency cells with cqDataFile cannot use a reasoning lane, and always merge the repo's own store  [OPEN]  (2-hop)
- #1712: logic/mappings: salvage the correspondence frontend — dsl/mappings cells as logic:Correspondence IR  [OPEN]  (2-hop)
- #1746: logic: non-entailment obligation discharge misses logic:instanceOf-typed obligations  [CLOSED]  (2-hop)
- #1750: cli: consistency-family commands need gate-able exit codes and machine-readable verdicts  [OPEN]  (2-hop)
- #1751: logic: TPTP-FOF export + external prover harness (whole-corpus FOL verdicts)  [OPEN]  (2-hop)
- #1752: slice-quality: an external-corpus mode for sacred/consumer slices outside the GMEOW checkout  [CLOSED]  (2-hop)
- #306: EPIC: Music — frame-relative musical content; every notation a lossy projection  [CLOSED]  (2-hop)
- #307: Music · Structural foundation (extension scaffold, WEMI specialization, derivation & genre)  [CLOSED]  (2-hop)
- #310: Music · Musical time (time frames, tempo maps, meter, polymeter vs polyrhythm, groove)  [CLOSED]  (2-hop)
- #1796: Two authored existential cycles through inverse properties cannot be certified by a skolem-chase termination certifier  [CLOSED]  (2-hop)
- #1797: Lift the chase termination ladder and Datalog-first scheduling into purrdf-datalog, then consume it  [OPEN]  (2-hop)
- #1793: fix(1789): specialize termination analysis without combinatorial products  [MERGED]  (2-hop)
- #1794: fix(1790): publish each world's own chase certificate and enforce the demonstrator classes  [CLOSED]  (2-hop)
- #1798: feat(1796): run the restricted chase Datalog-first  [MERGED]  (2-hop)
- #1799: feat(1796): certify restricted joint acyclicity  [MERGED]  (2-hop)
- #1800: fix(1795): sharpen termination-statement encoding and bind closed relations  [MERGED]  (2-hop)
- #1801: perf(1757): compute joint acyclicity in linear Horn propagation  [MERGED]  (2-hop)
- #1770: feat: certify and precompile the complete canon for consumer proof  [OPEN]  (2-hop)
- #1768: feat: add a first-class consumer prover entry point  [OPEN]  (2-hop)
- #1646: schemas: recover the developer-surface capability lost in the purrdf emitter cutover  [OPEN]  (2-hop)
- #164: Canonical private-use language tags with public BCP 47 projection  [CLOSED]  (2-hop)

## Prior attempts at this issue

Commits citing it (including inside squash merges):

- `cc47395c7 fix: preserve main acceptance runs and lower stale helper debt`
- `795612290 fix: admit complete actions within the shared cache budget`
- `bc70ca583 fix(ci): fetch locked inputs before the offline helper census`
- `edef801c6 fix(ci): require helper hygiene in the initial scheduling wave`
- `9fa28c6fe fix(logic): keep rejected decimals outside rational interpretation`
- `bc230b3f6 fix(pipeline): project term entailments from retained native results`
- `28817b370 fix(logic): preserve alternate calculus spellings`

Other branches named for it:

- None.

PRs citing it:

- #1801: perf(1757): compute joint acyclicity in linear Horn propagation  [MERGED]
- #1871: Preserve main acceptance runs and lower stale helper debt  [MERGED]  (this stage's own PR)
- #1791: perf(1757): make native reasoning fit and finish on the production corpus  [MERGED]
- #1885: fix: route imported template model types  [MERGED]
- #1882: Restore Turtle statement and namespace declarations  [MERGED]
- #1866: Admit complete compiler actions within the shared cache budget  [MERGED]  (this stage's own PR)
- #1876: Use validated ownership for model export documentation  [MERGED]
- #1837: fix(logic): preserve native evidence and portable reasoning identities  [MERGED]
- #1804: fix(slices): repair the corpus inconsistencies native reasoning exposed  [MERGED]
- #1785: feat(dev): helper census: one home per job, PurRDF's where it exists  [MERGED]
- #1758: fix(ci): admit transferred optimized producers  [MERGED]
- #1760: perf(logic): restore certified batch slice probes  [MERGED]
- #1759: perf(pipeline): schedule stages on completion  [MERGED]
- #1740: perf(logic): integrate the authenticated native pipeline and calculus  [MERGED]
- #1788: fix(1787): carry every logic: list to graph/logic in one encoding  [MERGED]
- #1784: deps: adopt purrdf 3.0 and enforce a major-3 substrate floor  [MERGED]

## Related items on the forge

- #1745: perf: adopt typed correspondence execution throughout the production pipeline  [OPEN]
- #1741: logic: complete typed correspondence execution and law-bearing source recovery  [OPEN]
- #1194: V4 — Equivalence-before-deletion: retire hand-authored shapes.ttl + structural ShEx  [CLOSED]
- #1518: perf(rust): optimize the native pipeline and make check critical path  [CLOSED]
- #1700: perf(test): 2x required-lane optimization with DAG caching  [CLOSED]
- #1655: repo hygiene: sweep stale process references and tighten the ref lint  [CLOSED]
- #629: EPIC — Remove rdflib repo-wide: port remaining graph logic to Rust + oxigraph (gmeow_validate), pyoxigraph-only Python seam  [CLOSED]
- #856: logic-compile: have gmeow_logic.compile_logic return a native Report directly (drop the Python dict forward)  [CLOSED]
- #1330: Upgrade to purrdf 0.3.0 and migrate slice tests to the bnode-capable Rust harness  [CLOSED]
- #840: P6 — purrdf: RdfEventSource/RdfEventSink seam  [CLOSED]
- #835: P1 — purrdf: SRP-decompose py_store.rs  [CLOSED]
- #713: T3 — Hypothetical / subjunctive operator (◇): test without committing  [CLOSED]
- #710: epic: Transaction Logic (Bonner–Kifer) — state-change, queries+updates+transactions unified in the logic: layer  [CLOSED]
- #1413: feat(logic): decide the withheld OWL 2 DL/Full fragment natively — complement refutation, qualified-cardinality counting, nominal/datatype value-space, xsd facet evaluation  [CLOSED]

## Recurrence

Counted from `GhpRsq-Defect-Class` trailers — these are tallies, not estimates:

- `none`: 44 prior instance(s)
- `missing-intrinsic-runtime`: 3 prior instance(s)
- `model-ownership`: 1 prior instance(s)
- `unsupported-consequent`: 1 prior instance(s)
- `https//blackcatinformatics.ca/gmeow/RecordedConsumerFailure`: 1 prior instance(s)

## Coverage

What was searched, and what was not:

- issue #1757 + 46 comment(s) via github
- link crawl: 56 issue(s) within 2 hop(s) of `#N` references
- forge search on ['perf/logic:', 'finish', 'merged']: 14 other item(s)
- GhpRsq-Defect-Class trailers: 5 distinct class(es) in the last 200 commits
- prior attempts: 7 commit(s) citing #1757 (second parents walked), 0 other branch(es) named for it
- PRs citing #1757: 16
- requirement candidates: 36 line(s); 22 comment(s) matched no rule
- referenced artifacts: 31 path(s) (23 on disk), 46 identifier(s), 5 workspace package(s), 15 URL(s)
