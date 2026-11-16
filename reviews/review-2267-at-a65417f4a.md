BLOCKED at a65417f4a

# Review of issue 2267 (finality vocabulary, memory-records) at a65417f4a

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2267-marks-on-an-agent-s-important-events`,
  branch `paudley/2267-marks-on-an-agent-s-important-events`, head
  `a65417f4a4f60e8b009d7d4be7879ea7fe837be1`, clean tree.
- `git fetch origin main` ran under the build lock. `origin/main` is `279ce7e40`, which is also the merge
  base, so the change is one commit on current main: 8 files, all under `slices/core/memory-records`.
- Requirement read in full: the issue body and both comments (the release-blocker note and the
  maintainer's split decision with its three acceptance items).
- Read: worktree `AGENTS.md`, Constitution Principles 9 and 10, `LOGIC-VALIDATION.md` (closure and
  required-path doctrine), the whole changed region of `module.ttl` and its trace siblings (lines
  846-1411), the example, both test files, the docs and design diffs plus the unchanged finality
  passages, the manifest, the 79 new entries of each catalog, the coder's stage logs and `hand-eval.md`,
  `crates/slicetest/src/{exec,stores,native_query,paths}.rs`, spec discovery in `repository.rs`, the
  pinned SPARQL parser (`purrdf-sparql-algebra` 4.0.0), and the gates named in the ledger table.

## Required findings

### R1. One structural assertion cannot parse: undeclared `rdfs:` prefix

`slices/core/memory-records/tests/structural.ttl:31-51` (`ex:saTraceFinalityRequiredRoles`). The pattern
declares only `PREFIX gmeow:` and `PREFIX logic:` (lines 33-34) and then uses `rdfs:Literal` on line 42:

```
gmeow:TraceMarksGivenUp logic:subClassOf [ logic:onProperty gmeow:givenUpReason ; logic:allValuesFrom rdfs:Literal ] .
```

What I read:

- The harness passes `saPattern` to the engine verbatim, with no prelude: `crates/slicetest/src/exec.rs`
  `run_ask` (714-727) calls `native_query::query` (`native_query.rs:169-185`).
- The pinned parser (Cargo.lock: `purrdf-sparql-algebra` 4.0.0) starts with no prefixes and returns a
  syntax error for an unknown one: `parser.rs:1630-1657` (`"undeclared prefix {prefix:?}"`), with its own
  test `undeclared_prefix_is_syntax_error` at 7605.

So this cell returns `saPattern error`, and `run_structural_file` fails the whole file's aggregate. This
is the assertion that pins the required roles of acceptance item 1. `hand-eval.md` states "Every query
declares each prefix it uses"; that is false for this cell. Fix: add
`PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>` to the pattern. The other eight patterns and the
three competency queries declare every prefix they use (checked one by one).

### R2. The frozen reviewed-coverage golden has no entry for either catalog this branch extends

`crates/docs/tests/po_parser_parity.rs:48-67` (`corpus_reviewed_coverage_matches_frozen_golden`) asserts
map equality between `crates/docs/tests/fixtures/reviewed_coverage_golden.json` and the producer
observation. `crates/docs-model/src/reviewed_coverage.rs:86-117` (`observe`) inserts one key per live
slice catalog, holding every entry that is non-fuzzy, non-empty and passes the integrity guard.

- The golden has 148 keys. Neither `slices/core/memory-records/i18n/fr.po` nor `.../zh.po` is among them
  (42 slice catalogs in 21 slices are absent; the golden was last changed 2026-10-21, these catalogs were
  added 2026-10-30).
- The branch adds 79 entries to each catalog (all non-fuzzy, non-empty, none copied from English) and
  does not touch the golden.

Answer to the question asked: yes, the 158 new keys must be in the golden for that test to pass, and the
branch did not add them. More than 158 are needed, because the entries these two catalogs already had on
main are missing too. As read, the comparison is already unequal at the merge base for this slice, so the
branch enlarges an existing drift and does not create it. It still cannot pass at this head. The
refresh is `make maint-refresh-reviewed-coverage-golden` (Makefile:1322), a maintainer target that
rewrites the whole file and so would also add the other 40 absent catalogs. Decision needed from the
operator: refresh here, or land a separate refresh first. I did not run the test.

### R3. A finality declaration can be valid with no claimant, and the watermark's producer scope is ambiguous

Principle 9 makes every value an attributed claim, and the decision says each record "records what a
recorder declared". What the module requires:

- `module.ttl:1350-1370`: required paths are ledger, producer and cut on `TraceWatermark`; ledger,
  producer and point on `TraceMarkFrontier`; reason on `TraceMarksGivenUp`. None of the three classes has
  a `wasAttributedTo` restriction or class-local closure entry. The sibling `TraceMark` does
  (`module.ttl:1037` and `1043`).
- `module.ttl:1278-1279` defines `traceFinalityProducer` as "not the agent that made the declaration" and
  says the declarer "is recorded separately through wasAttributedTo".

So the only required agent is explicitly not the claimant, and the claimant is optional. A
`TraceWatermark` with ledger, producer and packet and no attribution passes every authored check and
reads as a bare fact that marking is final. A `TraceMarksGivenUp` needs only a reason: no producer, no
declarer, no packet.

The same choice leaves the watermark's meaning undetermined:

- `module.ttl:1225` (definition): made "for one TracePacketLedger and one producer", yet "no mark will
  name such a packet" (unscoped).
- `module.ttl:1238` (frontier definition): a watermark "is a finality cut over the whole ledger".
- `module.ttl:1226` (scope note): the cut presupposes "every applicable producer frontier has been
  reached", which only matters if the cut covers other producers' marks.
- `module.ttl:1278`: the producer is the agent whose marks the declaration "is about".

A recorder cannot tell whether a watermark naming producer P finalizes P's marks or all marks, and that
is the reading on which it decides a discard. The issue's sentence is the global one; the acceptance
asks that a watermark "name the ledger and producer [it] belong[s] to", which reads as the owning
producer.

Fix, either form: (a) require `wasAttributedTo` on the three classes in the existing form
(`allValuesFrom gmeow:Agent` plus a class-local closure entry) and state in the watermark definition
whose marks it finalizes; or (b) define `traceFinalityProducer` as the declaring producer and say so.
Either touches the definitions and their two translations. If the operator rules that the acceptance
list is exhaustive and attribution stays optional, this becomes optional, but the watermark scope
sentence still needs to be made unambiguous.

## Optional findings

- **O1. `saTraceFinalityNoComparison` misses the likeliest regression.** `structural.ttl:53-65` binds
  `?term` to the six roles only. A later constraint that compares times and `logic:formalizes
  gmeow:TraceWatermark` (the class, as the sibling `TraceMarkPacketPresenceConstraint` formalizes its
  class) is not seen. Add the three classes to `VALUES`. The rationale's "a constraint added later that
  compares them fails this assertion" holds only for role-formalizing constraints.
- **O2. `saTraceFinalityRequiredRoles` does not pin the closure value.** After R1 is fixed, an entry with
  `logic:OpenWorldClosure` would still satisfy lines 43-49. Add `logic:closureValue
  logic:ClosedWorldClosure` to each.
- **O3. `saTraceFinalitySingleValue` mirrors the formula tree node for node** (`structural.ttl:109-135`).
  It does guard a boundary (term distinctness, not order), so it is defensible, but it will fail on any
  equivalent re-authoring.
- **O4. Two idioms for "at most one" in one module.** Siblings use `logic:maxQualifiedCardinality 1` with
  `logic:onClass` (for example `module.ttl:1049`, `1051`, `1053`); the new roles use a uniqueness
  `logic:Constraint`. Both are sanctioned forms and the constraint matches shipped precedent node for
  node (`slices/core/record-metadata/module.ttl:862-884`, `slices/core/inquiry/module.ttl:762-790`).
  The stated reason contrasts only with a functional axiom, not with the module's own cardinality idiom.
- **O5. Sortal.** The three classes are `logic:SubKind` directly under `gmeow:InformationObject`, which
  is a `logic:Category` (`slices/core/kernel/module.ttl:160-162`), so they have no Kind ancestor to
  supply identity. `MemoryBank` and `BodyDistillation` in this module are `logic:Kind` under the same
  parent; `TraceAnchor` and `TracePacketLedger` on main already carry the SubKind typing. The branch
  follows its nearest siblings; I found no gate that rejects it.
- **O6. Chinese terminology.** The existing catalog renders "trace" as 轨迹 (36 lines, never 跟踪:
  "Trace mark" is 轨迹标记). All new entries use 跟踪 (6 lines, never 轨迹: "Trace mark frontier" is
  跟踪标记前沿). One concept now has two names in one catalog.
- **O7. Translation nits.** French `traceFinalityProducer` useWhen renders "whose marking" as "de quel
  marquage" (which marking). French `traceFinalityLedger` definition inverts the relation ("dont l'ordre
  des paquets concerne une déclaration"). Chinese `TraceWatermark` howToUse renders "eligible for
  discard" as 才可被丢弃 (may be discarded), where the other entries use 具备被丢弃的资格; the
  permission reading is the one the decision excludes.
- **O8. "Each term says plainly".** The three classes and two constraints carry the no-order-check
  sentence. `traceFinalityLedger`, `traceFinalityProducer`, `givenUpPacket` and `givenUpReason` do not.
  I read "each term" in the decision as the three records, so this is optional; if it means all eleven
  subjects, four are short.
- **O9. `givenUpReason` has range `rdfs:Literal`** while its howToUse says to keep the language tag. The
  module's prose roles restrict to `rdf:langString`/`rdf:dirLangString`; an untagged string is accepted
  here.
- **O10. The guide does not link the worked example.** `docs.md:129-131` links the two older examples;
  `examples/trace-finality.ttl` is not linked from the new section.
- **O11. Wording.** `design/MEMORY-RECORDS.md:201-202` "state what remains undone" and `:247` "already use"
  are mild history wording. The "Deferred laws, and why" paragraph is what acceptance item 2 asks for
  and cites no tracker.

## What holds

- **Scope.** Nothing out of scope is delivered: no comparison primitive, no order law, no constraint that
  compares a time or a digest (the only relations in the two new formula trees are `rdf:type`, the role
  itself and `logic:termDistinct`). No Rust, no shape surface, no `owl:`, no guarded-vocabulary
  construct.
- **Issue and decision mapping.** Watermark, frontier, give-up and its reason are declared with
  definitions, scope notes, use/avoid/how notes, an inline example each and one worked instance each in
  `examples/trace-finality.ttl`. Never-backward, not-passing-a-frontier and give-up-first are stated as
  unchecked in the class scope notes, in `docs.md:378-384` and in the design note. Discard eligibility is
  conditional prose (`module.ttl:1230`, `docs.md:386-392`, design 272-276). I judge prose sufficient: the
  condition needs the deferred order comparison, the issue proposes no eligibility term, and the
  acceptance lists three records.
- **Reuse.** Existing near-names are different concepts: `gmeow:lossWatermark` (a commit coordinate of a
  query loss), `gmeow:LedgerFinalityStatus` (chain settlement), `logic:ActionableFrontier` (work
  frontier), `gmeow:actionWatermark` (rights). Minting is justified; ledger, packet, anchor and agent are
  reused.
- **Constraint forms.** Required paths are `logic:allValuesFrom` plus `logic:ClosureEntry` with
  `logic:onClass`, `logic:closureKey` and `logic:ClosedWorldClosure`, the form of `module.ttl:1039-1056`.
  The six property-global entries and the union-class domain have shipped precedent together
  (`slices/core/environment-records`, `slices/extensions/dreaming`). No unqualified cardinality, no
  functional axiom.
- **Hand evaluation.** On the example both uniqueness constraints hold (one cut, one point); adding a
  second distinct value makes the inner existential true and the implication false. Competency: each
  query runs over the merged modules plus the data file (`exec.rs:333-356`), no module holds an instance
  of the three classes, so each returns exactly the one expected row, including the `@en` reason
  literal. Structural, by hand over module (and examples for the last): vocabulary true, no-comparison
  false, derives-nothing false, no-rule-body false, no-formula-outside-constraint false, single-value
  true, not-functional false, example true. Each can fail. My results match `hand-eval.md` except R1.
  No question pairs `gmeow:cqDataFile` with a reasoning lane; the rule-body ban matches `logic:body` and
  `logic:negatedBody`.
- **i18n.** 79 English literals on the 11 new subjects; each has an entry in `fr.po` and `zh.po` whose
  parsed `msgid` equals the literal; none fuzzy, empty or copied; catalog diffs are additions only; every
  identifier in the English appears unchanged in both translations. I read 58 of the 79 in each language
  (every label, definition, scope note, message and avoidWhen, and each use/how note containing a
  negation, "declare" or "final"); negations and the declared-not-verified sense are preserved.
- **Syntax and prose.** `rapper -c` exits 0 on the four Turtle files and `prose-check` exits 0 on
  `docs.md` and the design note, both run by me at this head under the lock.
- **Evidence logs.** `checks.log` records head a65417f4a with an empty porcelain, `make lint` exit 0,
  four rapper passes, msgid PASS and prose-check 0. `msgid-prelim.log` is an earlier dirty-tree run and
  is superseded. No engine ran, as `hand-eval.md` says.

## Ledger table

| Gate or ledger | Applies | Branch satisfies | What I read |
|---|---|---|---|
| `crates/logic-compile/src/frontend/functional_carrier_ledger.txt` | No | n/a | Tracks properties bearing a functional `logic:characterizes` record (`frontend.rs:4419`); the branch adds none, and no memory-records property is listed |
| `crates/docs/tests/fixtures/reviewed_coverage_golden.json` | Yes | **No (R2)** | 148 keys, none for memory-records; equality test at `po_parser_parity.rs:48-67`; golden untouched |
| Diagnostic-kind census (`crates/validate/src/repo_static.rs:3540`) | No | n/a | Scans `define_diag_kind!` in `crates/*/src`; no Rust changed |
| Hand-authored `shapes.ttl` census (`repo_static.rs:762-900`) | No | n/a | The slice has no `shapes.ttl` and none is added |
| Projection-vocabulary ratchet (`gmeow:ProjectionCeilingCommitment`) | Only if residue is added | Yes, by reading | No ceiling record names this slice; the diff adds no SHACL, gUFO, bridge, RDFS-subsumption or alignment construct. Default for a slice with no record not determined |
| Slice-quality axis and tier floors (`slices/core/slice-quality-rubric/module.ttl`) | No | n/a | Zero occurrences of "memory" in the rubric: no `afc-` or tier-floor record. `axis_floor_for` (`dev_slice_quality.rs:1788-1808`) returns no floor outside the grounding GMN-1 default, so every axis is advisory. Coder's mirrors: prose 42/78 to 60/96, translation 1.0 to 1.0 |
| Term release authority (`metadata/releases/term-content-authority.json`) | No | n/a | Advanced only at a release boundary (Makefile:1472); sibling trace terms on main are not in it |
| Slice spec discovery | Automatic | Yes | `repository.rs:1099-1127` selects by file name under any `tests/`; 150 specs become 152; no pinned count or cell-name golden found in `crates/slicetest`, `crates/gmeow-dev-cli/src`, `crates/validate`, `tests` |
| Native-reasoning source list (`stores.rs:261-280`) | No | n/a | All three questions use `gmeow:reasoningNone` |
| Catalog `msgid` identity | Yes | Yes | 79 of 79 in each language |

## Not verified

- No harness, validator, reasoner, producer or gate ran. The three competency cells and nine structural
  cells were evaluated by hand only; R1 rests on reading the parser, not on a failing run.
- That the two constraints, the required paths, the property-global closures and the union domain lower
  and derive as intended. I rely on node-for-node precedent, not on the compiler's output.
- EL-safety as the reasoner judges it (`make reason-verify` not run).
- The outcome of the reviewed-coverage test (R2 is from source and the golden file).
- The projection ceiling default for a slice with no record, and whether any docs or catalog lint beyond
  `make lint` flags the new entries.
- Slice-quality mirrors were not re-run by me; no floor exists for them to test.
- Translation fidelity is one reader's judgment, not a native review; 21 entries per language were
  checked mechanically only.
- Hosted CI and `make check` for this head.
- Lock use: the fetch, `rapper` and `prose-check` ran under `flock .stage/build.lock`. Read-only `git`,
  `grep` and file reads ran without it, because another session held the lock for a long test run.

VERDICT: BLOCKED
